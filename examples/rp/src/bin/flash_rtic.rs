#![no_main]
#![no_std]

use defmt_rtt as _;

#[rtic::app(device = pac, peripherals = false, dispatchers = [XIP_IRQ, USBCTRL_IRQ])]
mod app {
    mod pac {
        pub const NVIC_PRIO_BITS: u8 = 2;
        pub use cortex_m_rt::interrupt;
        pub use embassy_rp::pac::{Interrupt as interrupt, *};
    }
    use embassy_rp::flash::{ERASE_SIZE, FLASH_BASE};
    use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
    use {defmt_rtt as _, panic_probe as _};

    const ADDR_OFFSET: u32 = 0x100000;
    const FLASH_SIZE: usize = 2 * 1024 * 1024;

    #[shared]
    struct SharedResources {}

    #[local]
    struct LocalResources {
        flash: embassy_rp::flash::Flash<'static, embassy_rp::peripherals::FLASH, FLASH_SIZE>,
    }

    #[init]
    fn init(_ctx: init::Context) -> (SharedResources, LocalResources, init::Monotonics()) {
        let p = embassy_rp::init(Default::default());

        (
            SharedResources {},
            LocalResources {
                flash: embassy_rp::flash::Flash::new(p.FLASH),
            },
            init::Monotonics(),
        )
    }

    #[idle(local = [flash])]
    fn idle(ctx: idle::Context) -> ! {
        let flash = ctx.local.flash;

        defmt::info!(">>>> [erase_write_sector]");
        let mut buf = [0u8; ERASE_SIZE];
        defmt::unwrap!(flash.read(ADDR_OFFSET, &mut buf));

        defmt::info!("Addr of flash block is {:x}", ADDR_OFFSET + FLASH_BASE as u32);
        defmt::info!("Contents start with {=[u8]}", buf[0..4]);

        defmt::unwrap!(flash.erase(ADDR_OFFSET, ADDR_OFFSET + ERASE_SIZE as u32));

        defmt::info!("Contents after erase starts with {=[u8]}", buf[0..4]);
        if buf.iter().any(|x| *x != 0xFF) {
            defmt::panic!("unexpected");
        }

        for b in buf.iter_mut() {
            *b = 0xDA;
        }

        defmt::unwrap!(flash.write(ADDR_OFFSET, &buf));

        defmt::unwrap!(flash.read(ADDR_OFFSET, &mut buf));
        defmt::info!("Contents after write starts with {=[u8]}", buf[0..4]);
        if buf.iter().any(|x| *x != 0xDA) {
            defmt::panic!("unexpected");
        }

        loop {}
    }
}
