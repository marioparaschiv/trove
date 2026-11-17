//! ureq plumbing for the model download: how a failed lookup and a stalled transfer are reported.

use std::{io, time::Duration};

use ureq::{
    config::Config as HttpConfig,
    http::Uri,
    unversioned::{
        resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver},
        transport::{Buffers, ConnectionDetails, Connector, NextTimeout, Transport},
    },
};

/// ureq reports a failed lookup as a plain I/O error, indistinguishable from a failed connection.
#[derive(Debug)]
pub(super) struct LookupFailedResolver(pub(super) DefaultResolver);

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
pub(super) struct IdleTimeout(pub(super) Duration);

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
pub(super) struct IdleTransport<T> {
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
