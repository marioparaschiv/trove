use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use ureq::{
    Agent,
    config::Config as HttpConfig,
    http::{Uri, header},
    unversioned::{
        resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
        transport::{
            Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
        },
    },
};

use crate::{config::Config, data_dir::DataDir};

pub const MODEL_REPO: &str = "ggml-org/embeddinggemma-2-GGUF";
pub const MODEL_REVISION: &str = "bfcd298762cc34d0357ece5ebdd31791a3a374d8";
pub const MODEL_FILE: &str = "embeddinggemma-2-Q8_0.gguf";
pub const MODEL_SIZE: u64 = 309_855_456;
pub const MODEL_SHA256: &str = "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135";

pub const MODELS_FOLDER: &str = "models";
pub const HF_ENDPOINT_ENV: &str = "HF_ENDPOINT";
const DEFAULT_HF_ENDPOINT: &str = "https://huggingface.co";

const MAX_FAILURES: u32 = 20;
const MAX_BAD_CHECKS: u32 = 3;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);
const MAX_REDIRECTS: u32 = 10;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// One file in a Hugging Face repository at a fixed revision, with its expected size and SHA-256.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFile {
    pub repo: String,
    pub revision: String,
    pub file: String,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
}

impl ModelFile {
    pub fn pinned() -> Self {
        Self {
            repo: MODEL_REPO.to_owned(),
            revision: MODEL_REVISION.to_owned(),
            file: MODEL_FILE.to_owned(),
            size: MODEL_SIZE,
            sha256: MODEL_SHA256.to_owned(),
        }
    }

    /// Where `trove setup` puts the model. A file at this path has passed its size and hash check.
    pub fn path_in(&self, data_dir: &DataDir) -> PathBuf {
        data_dir.path().join(MODELS_FOLDER).join(&self.file)
    }

    fn url(&self, endpoint: &str) -> String {
        format!(
            "{endpoint}/{}/resolve/{}/{}",
            self.repo, self.revision, self.file
        )
    }

    /// The layout `hf download` writes: `models--{owner}--{name}/snapshots/{revision}/{file}`.
    pub fn path_in_hf_cache(&self, hub: &Path) -> PathBuf {
        hub.join(format!("models--{}", self.repo.replace('/', "--")))
            .join("snapshots")
            .join(&self.revision)
            .join(&self.file)
    }
}

/// Progress of `setup`, serialised as one JSON line each by `trove setup --json`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    File {
        name: String,
        bytes: u64,
        from: Source,
    },
    Progress {
        name: String,
        bytes: u64,
        total: u64,
    },
    Waiting,
    Verifying {
        name: String,
    },
    Retry {
        message: String,
        seconds: f64,
    },
    Done {
        path: PathBuf,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Cache,
    Download,
}

#[derive(Debug, Error)]
pub enum SetupError {
    #[error("{HF_ENDPOINT_ENV} must be an http or https URL, got {0:?}")]
    BadEndpoint(String),
    #[error("failed to {action} {path}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{url} answered {status}")]
    Status { url: String, status: u16 },
    #[error("{name} failed to download {MAX_FAILURES} times")]
    TooManyFailures {
        name: String,
        #[source]
        last: TransferError,
    },
    #[error("{name} arrived damaged {MAX_BAD_CHECKS} times")]
    Damaged { name: String },
}

/// A failed download attempt that is worth trying again.
#[derive(Debug, Error)]
pub enum TransferError {
    #[error("request failed")]
    Http(#[from] ureq::Error),
    #[error("{url} answered {status}")]
    Status { url: String, status: u16 },
    #[error("{url} redirected without a usable Location")]
    BadRedirect { url: String },
    #[error("more than {MAX_REDIRECTS} redirects")]
    TooManyRedirects,
    #[error("asked to resume at byte {expected} but got range {got:?}")]
    WrongRange { expected: u64, got: String },
    #[error("download interrupted")]
    Read(#[source] io::Error),
    #[error("download stopped at {written} of {size} bytes")]
    StoppedEarly { written: u64, size: u64 },
}

enum Attempt {
    Complete,
    Offline,
    Failed(TransferError),
}

/// Puts `config.model` into the data folder: copied from the Hugging Face cache when it is there, otherwise
/// downloaded. Reports progress through `emit`, ending with `Done` or `Error`.
pub fn setup(
    data_dir: &DataDir,
    config: &Config,
    emit: &mut dyn FnMut(Event),
) -> Result<PathBuf, SetupError> {
    let result = install(data_dir, config, emit);

    match &result {
        Ok(path) => emit(Event::Done { path: path.clone() }),
        Err(error) => emit(Event::Error {
            message: describe(error),
        }),
    }

    result
}

fn install(
    data_dir: &DataDir,
    config: &Config,
    emit: &mut dyn FnMut(Event),
) -> Result<PathBuf, SetupError> {
    let model = &config.model;
    let path = model.path_in(data_dir);
    if path
        .try_exists()
        .map_err(|source| io_error("check", &path, source))?
    {
        return Ok(path);
    }

    let folder = data_dir.path().join(MODELS_FOLDER);
    fs::create_dir_all(&folder).map_err(|source| io_error("create", &folder, source))?;
    let partial = folder.join(format!("{}.partial", model.file));

    if let Some(cached) = hf_cache_copy(model)? {
        emit(Event::File {
            name: model.file.clone(),
            bytes: model.size,
            from: Source::Cache,
        });
        fs::copy(&cached, &partial).map_err(|source| io_error("copy", &cached, source))?;
    } else {
        let endpoint = hf_endpoint()?;
        emit(Event::File {
            name: model.file.clone(),
            bytes: model.size,
            from: Source::Download,
        });
        download(config, &model.url(&endpoint), &partial, emit)?;
    }

    fs::rename(&partial, &path).map_err(|source| io_error("rename", &partial, source))?;

    Ok(path)
}

/// The model in the Hugging Face cache, if it is there and passes its check.
fn hf_cache_copy(model: &ModelFile) -> Result<Option<PathBuf>, SetupError> {
    let Some(hub) = hf_hub_cache() else {
        return Ok(None);
    };

    let cached = model.path_in_hf_cache(&hub);
    match fs::metadata(&cached) {
        Ok(metadata) if metadata.len() == model.size => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(io_error("read", &cached, source)),
    }

    Ok(passes_check(&cached, model)?.then_some(cached))
}

pub fn hf_hub_cache() -> Option<PathBuf> {
    if let Some(hub) = std::env::var_os("HF_HUB_CACHE") {
        return Some(PathBuf::from(hub));
    }

    if let Some(home) = std::env::var_os("HF_HOME") {
        return Some(PathBuf::from(home).join("hub"));
    }

    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".cache/huggingface/hub"))
}

fn hf_endpoint() -> Result<String, SetupError> {
    let Some(endpoint) = std::env::var_os(HF_ENDPOINT_ENV) else {
        return Ok(DEFAULT_HF_ENDPOINT.to_owned());
    };

    let endpoint = endpoint
        .into_string()
        .map_err(|endpoint| SetupError::BadEndpoint(endpoint.to_string_lossy().into_owned()))?;

    match endpoint.parse::<Uri>() {
        Ok(uri) if matches!(uri.scheme_str(), Some("http" | "https")) => {
            Ok(endpoint.trim_end_matches('/').to_owned())
        }
        _ => Err(SetupError::BadEndpoint(endpoint)),
    }
}

fn download(
    config: &Config,
    url: &str,
    partial: &Path,
    emit: &mut dyn FnMut(Event),
) -> Result<(), SetupError> {
    let model = &config.model;
    let retry_delay = config.download_retry_delay;
    let agent = Agent::with_parts(
        Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build(),
        DefaultConnector::new().chain(IdleTimeout(config.download_idle_timeout)),
        LookupFailedResolver(DefaultResolver::default()),
    );

    let mut progress = Progress {
        name: &model.file,
        total: model.size,
        last: None,
    };
    let mut failures = 0;
    let mut bad_checks = 0;
    let mut waits = 0;

    loop {
        match fetch(&agent, url, partial, model.size, &mut progress, emit)? {
            Attempt::Complete => {
                waits = 0;
                emit(Event::Verifying {
                    name: model.file.clone(),
                });
                if passes_check(partial, model)? {
                    return Ok(());
                }

                fs::remove_file(partial).map_err(|source| io_error("delete", partial, source))?;
                bad_checks += 1;
                if bad_checks >= MAX_BAD_CHECKS {
                    return Err(SetupError::Damaged {
                        name: model.file.clone(),
                    });
                }

                emit(Event::Retry {
                    message: format!("{} arrived damaged", model.file),
                    seconds: 0.0,
                });
            }
            Attempt::Offline => {
                if waits == 0 {
                    emit(Event::Waiting);
                }

                waits += 1;
                thread::sleep(backoff(retry_delay, waits));
            }
            Attempt::Failed(error) => {
                waits = 0;
                failures += 1;
                if failures >= MAX_FAILURES {
                    return Err(SetupError::TooManyFailures {
                        name: model.file.clone(),
                        last: error,
                    });
                }

                let delay = backoff(retry_delay, failures);
                emit(Event::Retry {
                    message: describe(&error),
                    seconds: delay.as_secs_f64(),
                });
                thread::sleep(delay);
            }
        }
    }
}

/// `first`, doubling with each attempt, capped at a minute.
fn backoff(first: Duration, attempt: u32) -> Duration {
    first
        .saturating_mul(1 << attempt.saturating_sub(1).min(16))
        .min(MAX_RETRY_DELAY)
}

struct Progress<'a> {
    name: &'a str,
    total: u64,
    last: Option<Instant>,
}

impl Progress<'_> {
    fn report(&mut self, bytes: u64, emit: &mut dyn FnMut(Event)) {
        if self
            .last
            .is_some_and(|last| last.elapsed() < PROGRESS_INTERVAL)
        {
            return;
        }

        self.last = Some(Instant::now());
        emit(Event::Progress {
            name: self.name.to_owned(),
            bytes,
            total: self.total,
        });
    }
}

/// One try: appends to `partial` from where it ends. Errors are the ones retrying can't fix.
fn fetch(
    agent: &Agent,
    url: &str,
    partial: &Path,
    size: u64,
    progress: &mut Progress,
    emit: &mut dyn FnMut(Event),
) -> Result<Attempt, SetupError> {
    let mut written = match fs::metadata(partial) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(source) => return Err(io_error("read", partial, source)),
    };
    if written > size {
        written = 0;
    }

    if written == size {
        return Ok(Attempt::Complete);
    }

    let response = match get_following_redirects(agent, url, written)? {
        Ok(response) => response,
        Err(error) if is_offline(&error) => return Ok(Attempt::Offline),
        Err(error) => return Ok(Attempt::Failed(error)),
    };

    if response.status() == 206 {
        let range = response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();

        if !range.starts_with(&format!("bytes {written}-")) {
            fs::remove_file(partial).map_err(|source| io_error("delete", partial, source))?;
            return Ok(Attempt::Failed(TransferError::WrongRange {
                expected: written,
                got: range.to_owned(),
            }));
        }
    } else {
        written = 0;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(partial)
        .map_err(|source| io_error("open", partial, source))?;
    file.set_len(written)
        .and_then(|()| file.seek(SeekFrom::End(0)))
        .map_err(|source| io_error("write", partial, source))?;

    // One byte past the expected size is enough to tell that the file is too long.
    let mut body = response.into_body().into_reader().take(size - written + 1);
    let mut buffer = vec![0; 1 << 16];

    loop {
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Ok(Attempt::Failed(TransferError::Read(error))),
        };

        file.write_all(&buffer[..read])
            .map_err(|source| io_error("write", partial, source))?;
        written += read as u64;
        progress.report(written, emit);
    }

    if written < size {
        return Ok(Attempt::Failed(TransferError::StoppedEarly {
            written,
            size,
        }));
    }

    Ok(Attempt::Complete)
}

/// Follows redirects by hand so the `Range` header goes along to the CDN. Returns a 200 or 206 response.
fn get_following_redirects(
    agent: &Agent,
    url: &str,
    from: u64,
) -> Result<Result<ureq::http::Response<ureq::Body>, TransferError>, SetupError> {
    let mut url = url.to_owned();

    for _ in 0..=MAX_REDIRECTS {
        let mut request = agent.get(&url);
        if from > 0 {
            request = request.header(header::RANGE, format!("bytes={from}-"));
        }

        let response = match request.call() {
            Ok(response) => response,
            Err(error) => return Ok(Err(error.into())),
        };

        let status = response.status().as_u16();
        match status {
            200 | 206 => return Ok(Ok(response)),
            301 | 302 | 303 | 307 | 308 => match redirect_target(&url, &response) {
                Some(target) => url = target,
                None => return Ok(Err(TransferError::BadRedirect { url })),
            },
            429 | 500..=599 => return Ok(Err(TransferError::Status { url, status })),
            _ => return Err(SetupError::Status { url, status }),
        }
    }

    Ok(Err(TransferError::TooManyRedirects))
}

fn redirect_target(url: &str, response: &ureq::http::Response<ureq::Body>) -> Option<String> {
    let location = response.headers().get(header::LOCATION)?.to_str().ok()?;
    if location.starts_with("http://") || location.starts_with("https://") {
        return Some(location.to_owned());
    }

    if !location.starts_with('/') {
        return None;
    }

    let uri: Uri = url.parse().ok()?;
    Some(format!(
        "{}://{}{location}",
        uri.scheme_str()?,
        uri.authority()?
    ))
}

/// A name that can't be looked up or a network that can't be reached means there is no network yet, so setup
/// waits instead of counting a failure. A refused connection means the network works and the host is down.
fn is_offline(error: &TransferError) -> bool {
    let TransferError::Http(error) = error else {
        return false;
    };

    match error {
        ureq::Error::HostNotFound | ureq::Error::Timeout(ureq::Timeout::Resolve) => true,
        ureq::Error::Io(error) => matches!(
            error.kind(),
            io::ErrorKind::NetworkUnreachable
                | io::ErrorKind::HostUnreachable
                | io::ErrorKind::NetworkDown
        ),
        _ => false,
    }
}

/// ureq reports a failed lookup as a plain I/O error, indistinguishable from a failed connection.
#[derive(Debug)]
struct LookupFailedResolver(DefaultResolver);

impl Resolver for LookupFailedResolver {
    fn resolve(
        &self,
        uri: &Uri,
        config: &HttpConfig,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        match self.0.resolve(uri, config, timeout) {
            Err(ureq::Error::Io(_)) => Err(ureq::Error::HostNotFound),
            result => result,
        }
    }
}

/// Fails any wait for data longer than the idle timeout, so a stalled transfer becomes a retry instead of a hang.
/// ureq's own timeouts only bound whole phases of a request.
#[derive(Debug)]
struct IdleTimeout(Duration);

impl<In: Transport> Connector<In> for IdleTimeout {
    type Out = IdleTransport<In>;

    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| IdleTransport {
            inner,
            idle: self.0,
        }))
    }
}

#[derive(Debug)]
struct IdleTransport<T> {
    inner: T,
    idle: Duration,
}

impl<T: Transport> Transport for IdleTransport<T> {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        if *timeout.after <= self.idle {
            return self.inner.await_input(timeout);
        }

        let idle = NextTimeout {
            after: self.idle.into(),
            reason: timeout.reason,
        };

        match self.inner.await_input(idle) {
            Err(ureq::Error::Timeout(_)) => Err(ureq::Error::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("no data for {}", humantime::format_duration(self.idle)),
            ))),
            result => result,
        }
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

fn passes_check(path: &Path, model: &ModelFile) -> Result<bool, SetupError> {
    let read_error = |source| io_error("read", path, source);
    let mut file = File::open(path).map_err(read_error)?;
    if file.metadata().map_err(read_error)?.len() != model.size {
        return Ok(false);
    }

    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 8 << 20];
    loop {
        let read = file.read(&mut buffer).map_err(read_error)?;
        if read == 0 {
            break;
        }

        hasher.update(&buffer[..read]);
    }

    Ok(hex::encode(hasher.finalize()) == model.sha256)
}

fn io_error(action: &'static str, path: &Path, source: io::Error) -> SetupError {
    SetupError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn describe(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();

    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }

    message
}
