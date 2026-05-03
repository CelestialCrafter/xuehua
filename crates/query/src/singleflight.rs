use std::{
    pin::pin,
    sync::atomic::{AtomicBool, Ordering},
};

use tokio::sync::Notify;

#[derive(Debug, Default)]
pub struct SingleFlight {
    notify: Notify,
    inflight: AtomicBool,
}

#[must_use]
#[derive(Debug)]
pub enum FlightRole<'a> {
    Pilot(FlightGuard<'a>),
    Passenger,
}

impl SingleFlight {
    pub async fn takeoff(&self) -> FlightRole<'_> {
        // switch to active if no one else is computing
        let exchange =
            self.inflight
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed);

        match exchange {
            Ok(_) => FlightRole::Pilot(FlightGuard(self)),
            Err(_) => {
                let mut notified = pin!(self.notify.notified());
                notified.as_mut().enable();

                // prevent deadlock if the pilot exited while we were enabling
                if self.inflight.load(Ordering::Acquire) {
                    notified.await;
                }

                FlightRole::Passenger
            }
        }
    }

    pub async fn pilot(&self) -> FlightGuard<'_> {
        loop {
            if let FlightRole::Pilot(guard) = self.takeoff().await {
                break guard;
            }
        }
    }
}

pub struct PilotToken {
    _private: (),
}

#[must_use]
#[derive(Debug)]
pub struct FlightGuard<'a>(&'a SingleFlight);

impl FlightGuard<'_> {
    pub fn tokenize(self) -> PilotToken {
        std::mem::forget(self);
        PilotToken { _private: () }
    }

    pub fn untokenize(flight: &SingleFlight, _token: PilotToken) -> FlightGuard<'_> {
        FlightGuard(flight)
    }
}

impl Drop for FlightGuard<'_> {
    fn drop(&mut self) {
        self.0.inflight.store(false, Ordering::Release);
        self.0.notify.notify_waiters();
    }
}
