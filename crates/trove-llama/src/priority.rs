//! How an embedding call shares the CPU: its thread count, a low-priority ggml thread pool and the OS priority of the
//! embedding thread.

use std::{ffi::c_void, io, ptr::NonNull, thread};

use crate::{LlamaError, bindings::*};

// More threads than this get slower on hybrid CPUs whose slow cores hold the fast ones back (DigUp's measurements).
const MAX_THREADS: usize = 8;

// The usual `nice` step: a background embedding still gets the CPU, but the user's programs get it first.
#[cfg(target_os = "linux")]
const BACKGROUND_NICE: i32 = 10;

/// How an embedding call shares the CPU with the rest of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// Up to 8 threads at normal priority.
    Normal,
    /// A quarter of the cores, at most 4 threads, at below-normal OS priority.
    Low,
}

impl Priority {
    pub(crate) fn threads(self) -> usize {
        let cores = thread::available_parallelism().map_or(1, |count| count.get());
        match self {
            Self::Normal => cores.min(MAX_THREADS),
            Self::Low => (cores / 4).clamp(1, MAX_THREADS / 2),
        }
    }
}

/// A ggml CPU thread pool, freed on drop.
pub(crate) struct Threadpool {
    pub(crate) pool: NonNull<ggml_threadpool>,
    free: unsafe extern "C" fn(ggml_threadpool_t),
}

impl Threadpool {
    /// `threads` workers, started from the calling thread, at ggml's low priority and without busy-waiting between
    /// graphs.
    pub(crate) fn low_priority(threads: i32) -> Result<Self, LlamaError> {
        type New = unsafe extern "C" fn(*mut ggml_threadpool_params) -> ggml_threadpool_t;
        type Free = unsafe extern "C" fn(ggml_threadpool_t);

        // The CPU backend is loaded at run time on Windows and Linux, so its thread pool functions are looked up.
        // SAFETY: the backends are initialised (a context exists), the names are NUL-terminated, and a non-null result
        // is the named function, whose signature is ggml-cpu.h's.
        let (new, free) = unsafe {
            let cpu = ggml_backend_reg_by_name(c"CPU".as_ptr());
            if cpu.is_null() {
                return Err(LlamaError::NoThreadpool);
            }
            let new = ggml_backend_reg_get_proc_address(cpu, c"ggml_threadpool_new".as_ptr());
            let free = ggml_backend_reg_get_proc_address(cpu, c"ggml_threadpool_free".as_ptr());
            if new.is_null() || free.is_null() {
                return Err(LlamaError::NoThreadpool);
            }
            (
                std::mem::transmute::<*mut c_void, New>(new),
                std::mem::transmute::<*mut c_void, Free>(free),
            )
        };

        // SAFETY: plain value struct from ggml's defaults.
        let mut params = unsafe { ggml_threadpool_params_default(threads) };
        // Windows threads don't take their creator's priority, so ggml lowers each worker it starts.
        params.prio = GGML_SCHED_PRIO_LOW;
        params.poll = 0;

        // SAFETY: `params` is valid for the call, which copies it.
        let pool = NonNull::new(unsafe { new(&mut params) }).ok_or(LlamaError::CreateThreadpool)?;
        Ok(Self { pool, free })
    }
}

impl Drop for Threadpool {
    fn drop(&mut self) {
        // SAFETY: created in `low_priority`, no context uses it any more, and freed only here.
        unsafe { (self.free)(self.pool.as_ptr()) };
    }
}

/// Lowers the calling thread below normal priority until it ends. Threads it starts take that over on Linux and macOS.
pub(crate) fn lower_this_thread() -> Result<(), LlamaError> {
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn setpriority(which: i32, who: u32, prio: i32) -> i32;
        }
        const PRIO_PROCESS: i32 = 0;

        // On Linux the nice value is per thread, and process 0 is the calling thread.
        // SAFETY: plain system call.
        if unsafe { setpriority(PRIO_PROCESS, 0, BACKGROUND_NICE) } != 0 {
            let error = io::Error::last_os_error();
            // Refused only as a raise: a trove started under a higher nice value is already lower than this.
            if error.kind() != io::ErrorKind::PermissionDenied {
                return Err(LlamaError::LowerPriority(error));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        const QOS_CLASS_UTILITY: u32 = 0x11;

        // SAFETY: plain call on the calling thread.
        let error = unsafe { pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0) };
        if error != 0 {
            return Err(LlamaError::LowerPriority(io::Error::from_raw_os_error(
                error,
            )));
        }
    }

    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut c_void;
            fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;

        // SAFETY: the pseudo handle of the calling thread is always valid.
        if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } == 0 {
            return Err(LlamaError::LowerPriority(io::Error::last_os_error()));
        }
    }

    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{fs, thread};

    use super::{BACKGROUND_NICE, lower_this_thread};

    fn nice_of_this_thread() -> i32 {
        let stat = fs::read_to_string("/proc/thread-self/stat").unwrap();
        // The fields after the parenthesised command name; the nice value is the 19th field of the line.
        let fields: Vec<&str> = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .collect();
        fields[16].parse().unwrap()
    }

    #[test]
    fn a_lowered_thread_and_the_threads_it_starts_run_niced_and_the_caller_keeps_its_priority() {
        let caller = nice_of_this_thread();
        let (lowered, started) = thread::spawn(|| {
            lower_this_thread().unwrap();
            (
                nice_of_this_thread(),
                thread::spawn(nice_of_this_thread).join().unwrap(),
            )
        })
        .join()
        .unwrap();

        let expected = caller.max(BACKGROUND_NICE);
        assert_eq!(lowered, expected);
        assert_eq!(started, expected);
        assert_eq!(nice_of_this_thread(), caller);
    }
}
