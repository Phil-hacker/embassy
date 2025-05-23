//! Builtin temperature sensor driver.

use core::future::poll_fn;
use core::task::Poll;

use embassy_hal_internal::drop::OnDrop;
use embassy_sync::waitqueue::AtomicWaker;
use fixed::types::I30F2;

use crate::interrupt::InterruptExt;
use crate::peripherals::TEMP;
use crate::{interrupt, pac, Peri};

/// Interrupt handler.
pub struct InterruptHandler {
    _private: (),
}

impl interrupt::typelevel::Handler<interrupt::typelevel::TEMP> for InterruptHandler {
    unsafe fn on_interrupt() {
        let r = pac::TEMP;
        r.intenclr().write(|w| w.set_datardy(true));
        WAKER.wake();
    }
}

/// Builtin temperature sensor driver.
pub struct Temp<'d> {
    _peri: Peri<'d, TEMP>,
}

static WAKER: AtomicWaker = AtomicWaker::new();

impl<'d> Temp<'d> {
    /// Create a new temperature sensor driver.
    pub fn new(
        _peri: Peri<'d, TEMP>,
        _irq: impl interrupt::typelevel::Binding<interrupt::typelevel::TEMP, InterruptHandler> + 'd,
    ) -> Self {
        // Enable interrupt that signals temperature values
        interrupt::TEMP.unpend();
        unsafe { interrupt::TEMP.enable() };

        Self { _peri }
    }

    /// Perform an asynchronous temperature measurement. The returned future
    /// can be awaited to obtain the measurement.
    ///
    /// If the future is dropped, the measurement is cancelled.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use embassy_nrf::{bind_interrupts, temp};
    /// use embassy_nrf::temp::Temp;
    ///
    /// bind_interrupts!(struct Irqs {
    ///     TEMP => temp::InterruptHandler;
    /// });
    ///
    /// # async {
    /// # let p: embassy_nrf::Peripherals = todo!();
    /// let mut t = Temp::new(p.TEMP, Irqs);
    /// let v: u16 = t.read().await.to_num::<u16>();
    /// # };
    /// ```
    pub async fn read(&mut self) -> I30F2 {
        // In case the future is dropped, stop the task and reset events.
        let on_drop = OnDrop::new(|| {
            let t = Self::regs();
            t.tasks_stop().write_value(1);
            t.events_datardy().write_value(0);
        });

        let t = Self::regs();
        t.intenset().write(|w| w.set_datardy(true));
        t.tasks_start().write_value(1);

        let value = poll_fn(|cx| {
            WAKER.register(cx.waker());
            if t.events_datardy().read() == 0 {
                Poll::Pending
            } else {
                t.events_datardy().write_value(0);
                let raw = t.temp().read();
                Poll::Ready(I30F2::from_bits(raw as i32))
            }
        })
        .await;
        on_drop.defuse();
        value
    }

    fn regs() -> pac::temp::Temp {
        pac::TEMP
    }
}
