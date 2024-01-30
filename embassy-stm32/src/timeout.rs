/// Error returned by [`with_timeout`] on timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TimeoutError;

#[derive(Copy, Clone)]
pub(crate) struct Timeout {
    #[cfg(feature = "time")]
    pub deadline: embassy_time::Instant,
}

#[allow(dead_code)]
impl Timeout {
    #[inline]
    pub fn check(&self) -> Result<(), TimeoutError> {
        #[cfg(feature = "time")]
        if embassy_time::Instant::now() > self.deadline {
            error!("TIMEOUT!");
            return Err(TimeoutError);
        }

        Ok(())
    }

    #[inline]
    pub async fn with<F: futures::Future>(self, fut: F) -> Result<F::Output, TimeoutError> {
        #[cfg(feature = "time")]
        return embassy_time::with_timeout(self.deadline - embassy_time::Instant::now(), fut)
            .await
            .map_err(|_| TimeoutError);

        #[cfg(not(feature = "time"))]
        Ok(fut.await)
    }
}
