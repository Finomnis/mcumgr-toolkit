use strum_macros::{Display, FromRepr};

/// See [`errno.h`](https://github.com/zephyrproject-rtos/zephyr/blob/main/lib/libc/minimal/include/errno.h).
#[derive(FromRepr, Display, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
#[allow(non_camel_case_types)]
pub enum Errno {
    /** Not owner */
    EPERM = 1,
    /** No such file or directory */
    ENOENT = 2,
    /** No such context */
    ESRCH = 3,
    /** Interrupted system call */
    EINTR = 4,
    /** I/O error */
    EIO = 5,
    /** No such device or address */
    ENXIO = 6,
    /** Arg list too long */
    E2BIG = 7,
    /** Exec format error */
    ENOEXEC = 8,
    /** Bad file number */
    EBADF = 9,
    /** No children */
    ECHILD = 10,
    /** No more contexts */
    EAGAIN = 11,
    /** Not enough core */
    ENOMEM = 12,
    /** Permission denied */
    EACCES = 13,
    /** Bad address */
    EFAULT = 14,
    /** Block device required */
    ENOTBLK = 15,
    /** Mount device busy */
    EBUSY = 16,
    /** File exists */
    EEXIST = 17,
    /** Cross-device link */
    EXDEV = 18,
    /** No such device */
    ENODEV = 19,
    /** Not a directory */
    ENOTDIR = 20,
    /** Is a directory */
    EISDIR = 21,
    /** Invalid argument */
    EINVAL = 22,
    /** File table overflow */
    ENFILE = 23,
    /** Too many open files */
    EMFILE = 24,
    /** Not a typewriter */
    ENOTTY = 25,
    /** Text file busy */
    ETXTBSY = 26,
    /** File too large */
    EFBIG = 27,
    /** No space left on device */
    ENOSPC = 28,
    /** Illegal seek */
    ESPIPE = 29,
    /** Read-only file system */
    EROFS = 30,
    /** Too many links */
    EMLINK = 31,
    /** Broken pipe */
    EPIPE = 32,
    /** Argument too large */
    EDOM = 33,
    /** Result too large */
    ERANGE = 34,
    /** Unexpected message type */
    ENOMSG = 35,
    /** Resource deadlock avoided */
    EDEADLK = 45,
    /** No locks available */
    ENOLCK = 46,
    /** STREAMS device required */
    ENOSTR = 60,
    /** Missing expected message data */
    ENODATA = 61,
    /** STREAMS timeout occurred */
    ETIME = 62,
    /** Insufficient memory */
    ENOSR = 63,
    /** Generic STREAMS error */
    EPROTO = 71,
    /** Invalid STREAMS message */
    EBADMSG = 77,
    /** Function not implemented */
    ENOSYS = 88,
    /** Directory not empty */
    ENOTEMPTY = 90,
    /** File name too long */
    ENAMETOOLONG = 91,
    /** Too many levels of symbolic links */
    ELOOP = 92,
    /** Operation not supported on socket */
    EOPNOTSUPP = 95,
    /** Protocol family not supported */
    EPFNOSUPPORT = 96,
    /** Connection reset by peer */
    ECONNRESET = 104,
    /** No buffer space available */
    ENOBUFS = 105,
    /** Addr family not supported */
    EAFNOSUPPORT = 106,
    /** Protocol wrong type for socket */
    EPROTOTYPE = 107,
    /** Socket operation on non-socket */
    ENOTSOCK = 108,
    /** Protocol not available */
    ENOPROTOOPT = 109,
    /** Can't send after socket shutdown */
    ESHUTDOWN = 110,
    /** Connection refused */
    ECONNREFUSED = 111,
    /** Address already in use */
    EADDRINUSE = 112,
    /** Software caused connection abort */
    ECONNABORTED = 113,
    /** Network is unreachable */
    ENETUNREACH = 114,
    /** Network is down */
    ENETDOWN = 115,
    /** Connection timed out */
    ETIMEDOUT = 116,
    /** Host is down */
    EHOSTDOWN = 117,
    /** No route to host */
    EHOSTUNREACH = 118,
    /** Operation now in progress */
    EINPROGRESS = 119,
    /** Operation already in progress */
    EALREADY = 120,
    /** Destination address required */
    EDESTADDRREQ = 121,
    /** Message size */
    EMSGSIZE = 122,
    /** Protocol not supported */
    EPROTONOSUPPORT = 123,
    /** Socket type not supported */
    ESOCKTNOSUPPORT = 124,
    /** Can't assign requested address */
    EADDRNOTAVAIL = 125,
    /** Network dropped connection on reset */
    ENETRESET = 126,
    /** Socket is already connected */
    EISCONN = 127,
    /** Socket is not connected */
    ENOTCONN = 128,
    /** Too many references: can't splice */
    ETOOMANYREFS = 129,
    /** Unsupported value */
    ENOTSUP = 134,
    /** Illegal byte sequence */
    EILSEQ = 138,
    /** Value overflow */
    EOVERFLOW = 139,
    /** Operation canceled */
    ECANCELED = 140,
}

impl Errno {
    /// Converts a raw errno error code to a string
    pub fn errno_to_string(err: i32) -> String {
        if err >= 0 {
            "EOK".to_string()
        } else if let Some(err_enum) = Self::from_repr(-err) {
            format!("{err_enum}")
        } else {
            format!("EUNKNOWN({err})")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_errno_to_string_success() {
        assert_eq!(Errno::errno_to_string(0), "EOK");
        assert_eq!(Errno::errno_to_string(1), "EOK");
    }

    #[test]
    fn test_errno_to_string_known_codes() {
        assert_eq!(Errno::errno_to_string(-1), "EPERM");
        assert_eq!(Errno::errno_to_string(-2), "ENOENT");
        assert_eq!(Errno::errno_to_string(-22), "EINVAL");
    }

    #[test]
    fn test_errno_to_string_unknown_code() {
        assert_eq!(Errno::errno_to_string(-999), "EUNKNOWN(-999)");
    }
}
