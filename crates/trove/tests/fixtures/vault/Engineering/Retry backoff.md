# Retry backoff

When a remote call fails, the client should not hammer the server again at once.

## Exponential delays

Double the wait after each failed attempt: 100 ms, 200 ms, 400 ms and so on, up to a cap of 30 seconds. Give up after a fixed budget of attempts and report the error.

## Jitter

Add randomness to each delay so that thousands of clients that failed together do not all come back at the same moment and knock the service over again.
