pub fn retry_with_delays_logged<T, E, F, const N: usize>(
    op_name: &str,
    delays_secs: [u64; N],
    mut op: F,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: ::std::fmt::Display,
{
    let total_attempts = N + 1;

    for (retry_index, delay_secs) in delays_secs.into_iter().enumerate() {
        let attempt = retry_index + 1;

        match op() {
            Ok(value) => {
                if attempt > 1 {
                    ::log::info!(
                        "retry: '{}' succeeded on attempt {}/{}",
                        op_name,
                        attempt,
                        total_attempts
                    );
                }
                return Ok(value);
            }
            Err(err) => {
                ::log::warn!(
                    "retry: '{}' failed on attempt {}/{}: {}. Retrying in {}s",
                    op_name,
                    attempt,
                    total_attempts,
                    err,
                    delay_secs
                );
                ::std::thread::sleep(::std::time::Duration::from_secs(delay_secs));
            }
        }
    }

    match op() {
        Ok(value) => {
            if total_attempts > 1 {
                ::log::info!(
                    "retry: '{}' succeeded on attempt {}/{}",
                    op_name,
                    total_attempts,
                    total_attempts
                );
            }
            Ok(value)
        }
        Err(err) => {
            ::log::warn!(
                "retry: '{}' failed on final attempt {}/{}: {}",
                op_name,
                total_attempts,
                total_attempts,
                err
            );
            Err(err)
        }
    }
}

#[macro_export]
macro_rules! retry {
    ($name:expr, $op:expr $(,)?) => {{
        $crate::macros::retry_with_delays_logged(
            $name,
            [5_u64, 10_u64, 20_u64],
            $op,
        )
    }};
    (delays = [$($delay:expr),+ $(,)?], $name:expr, $op:expr $(,)?) => {{
        $crate::macros::retry_with_delays_logged(
            $name,
            [$($delay),+],
            $op,
        )
    }};
}
