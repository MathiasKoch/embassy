#![macro_use]

use crate::gpio::{AnyPin, Pin};
use crate::pac::gpio::vals::{Afr, Moder};
use crate::pac::gpio::Gpio;
use crate::pac::spi;
use crate::spi::{ByteOrder, Config, Instance, MisoPin, MosiPin, SckPin, WordSize};
use crate::time::Hertz;
use core::marker::PhantomData;
use embassy::util::Unborrow;
use embassy_extras::unborrow;
pub use embedded_hal::spi::{Mode, Phase, Polarity, MODE_0, MODE_1, MODE_2, MODE_3};

impl WordSize {
    fn ds(&self) -> spi::vals::Ds {
        match self {
            WordSize::EightBit => spi::vals::Ds::EIGHTBIT,
            WordSize::SixteenBit => spi::vals::Ds::SIXTEENBIT,
        }
    }

    fn frxth(&self) -> spi::vals::Frxth {
        match self {
            WordSize::EightBit => spi::vals::Frxth::QUARTER,
            WordSize::SixteenBit => spi::vals::Frxth::HALF,
        }
    }
}

pub struct Spi<'d, T: Instance> {
    //peri: T,
    sck: AnyPin,
    mosi: AnyPin,
    miso: AnyPin,
    phantom: PhantomData<&'d mut T>,
}

impl<'d, T: Instance> Spi<'d, T> {
    pub fn new<F>(
        pclk: Hertz,
        peri: impl Unborrow<Target = T> + 'd,
        sck: impl Unborrow<Target = impl SckPin<T>>,
        mosi: impl Unborrow<Target = impl MosiPin<T>>,
        miso: impl Unborrow<Target = impl MisoPin<T>>,
        freq: F,
        config: Config,
    ) -> Self
    where
        F: Into<Hertz>,
    {
        unborrow!(peri);
        unborrow!(sck, mosi, miso);

        unsafe {
            Self::configure_pin(sck.block(), sck.pin() as _, sck.af());
            Self::configure_pin(mosi.block(), mosi.pin() as _, mosi.af());
            Self::configure_pin(miso.block(), miso.pin() as _, miso.af());
        }

        let sck = sck.degrade();
        let mosi = mosi.degrade();
        let miso = miso.degrade();

        unsafe {
            T::regs().cr2().write(|w| {
                w.set_ssoe(false);
            });
        }

        let br = Self::compute_baud_rate(pclk, freq.into());

        unsafe {
            T::regs().cr1().write(|w| {
                w.set_cpha(
                    match config.mode.phase == Phase::CaptureOnSecondTransition {
                        true => spi::vals::Cpha::SECONDEDGE,
                        false => spi::vals::Cpha::FIRSTEDGE,
                    },
                );
                w.set_cpol(match config.mode.polarity == Polarity::IdleHigh {
                    true => spi::vals::Cpol::IDLEHIGH,
                    false => spi::vals::Cpol::IDLELOW,
                });

                w.set_mstr(spi::vals::Mstr::MASTER);
                w.set_br(spi::vals::Br(br));
                w.set_spe(true);
                w.set_lsbfirst(match config.byte_order {
                    ByteOrder::LsbFirst => spi::vals::Lsbfirst::LSBFIRST,
                    ByteOrder::MsbFirst => spi::vals::Lsbfirst::MSBFIRST,
                });
                w.set_ssi(true);
                w.set_ssm(true);
                w.set_crcen(false);
                w.set_bidimode(spi::vals::Bidimode::UNIDIRECTIONAL);
            });
        }

        Self {
            //peri,
            sck,
            mosi,
            miso,
            phantom: PhantomData,
        }
    }

    unsafe fn configure_pin(block: Gpio, pin: usize, af_num: u8) {
        let (afr, n_af) = if pin < 8 { (0, pin) } else { (1, pin - 8) };
        block.moder().modify(|w| w.set_moder(pin, Moder::ALTERNATE));
        block.afr(afr).modify(|w| w.set_afr(n_af, Afr(af_num)));
    }

    unsafe fn unconfigure_pin(block: Gpio, pin: usize) {
        block.moder().modify(|w| w.set_moder(pin, Moder::ANALOG));
    }

    fn compute_baud_rate(clocks: Hertz, freq: Hertz) -> u8 {
        match clocks.0 / freq.0 {
            0 => unreachable!(),
            1..=2 => 0b000,
            3..=5 => 0b001,
            6..=11 => 0b010,
            12..=23 => 0b011,
            24..=39 => 0b100,
            40..=95 => 0b101,
            96..=191 => 0b110,
            _ => 0b111,
        }
    }

    fn set_word_size(word_size: WordSize) {
        unsafe {
            T::regs().cr2().write(|w| {
                w.set_ds(word_size.ds());
                w.set_frxth(word_size.frxth());
            });
        }
    }
}

impl<'d, T: Instance> Drop for Spi<'d, T> {
    fn drop(&mut self) {
        unsafe {
            Self::unconfigure_pin(self.sck.block(), self.sck.pin() as _);
            Self::unconfigure_pin(self.mosi.block(), self.mosi.pin() as _);
            Self::unconfigure_pin(self.miso.block(), self.miso.pin() as _);
        }
    }
}

pub enum Error {
    Framing,
    Crc,
    Overrun,
}

impl<'d, T: Instance> embedded_hal::blocking::spi::Write<u8> for Spi<'d, T> {
    type Error = Error;

    fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
        Self::set_word_size(WordSize::EightBit);
        let regs = T::regs();

        for word in words.iter() {
            while unsafe { !regs.sr().read().txe() } {
                // spin
            }
            unsafe {
                regs.dr().write(|reg| reg.0 = *word as u32);
            }
            loop {
                let sr = unsafe { regs.sr().read() };
                if sr.fre() {
                    return Err(Error::Framing);
                }
                if sr.ovr() {
                    return Err(Error::Overrun);
                }
                if sr.crcerr() {
                    return Err(Error::Crc);
                }
                if !sr.txe() {
                    // loop waiting for TXE
                }
            }
        }

        Ok(())
    }
}

impl<'d, T: Instance> embedded_hal::blocking::spi::Transfer<u8> for Spi<'d, T> {
    type Error = Error;

    fn transfer<'w>(&mut self, words: &'w mut [u8]) -> Result<&'w [u8], Self::Error> {
        Self::set_word_size(WordSize::EightBit);
        let regs = T::regs();

        for word in words.iter_mut() {
            while unsafe { !regs.sr().read().txe() } {
                // spin
            }
            unsafe {
                regs.dr().write(|reg| reg.0 = *word as u32);
            }
            while unsafe { !regs.sr().read().rxne() } {
                // spin waiting for inbound to shift in.
            }
            *word = unsafe { regs.dr().read().0 as u8 };
            let sr = unsafe { regs.sr().read() };
            if sr.fre() {
                return Err(Error::Framing);
            }
            if sr.ovr() {
                return Err(Error::Overrun);
            }
            if sr.crcerr() {
                return Err(Error::Crc);
            }
        }

        Ok(words)
    }
}

impl<'d, T: Instance> embedded_hal::blocking::spi::Write<u16> for Spi<'d, T> {
    type Error = Error;

    fn write(&mut self, words: &[u16]) -> Result<(), Self::Error> {
        Self::set_word_size(WordSize::SixteenBit);
        let regs = T::regs();

        for word in words.iter() {
            while unsafe { !regs.sr().read().txe() } {
                // spin
            }
            unsafe {
                regs.dr().write(|reg| reg.0 = *word as u32);
            }
            loop {
                let sr = unsafe { regs.sr().read() };
                if sr.fre() {
                    return Err(Error::Framing);
                }
                if sr.ovr() {
                    return Err(Error::Overrun);
                }
                if sr.crcerr() {
                    return Err(Error::Crc);
                }
                if !sr.txe() {
                    // loop waiting for TXE
                }
            }
        }

        Ok(())
    }
}

impl<'d, T: Instance> embedded_hal::blocking::spi::Transfer<u16> for Spi<'d, T> {
    type Error = Error;

    fn transfer<'w>(&mut self, words: &'w mut [u16]) -> Result<&'w [u16], Self::Error> {
        Self::set_word_size(WordSize::SixteenBit);
        let regs = T::regs();

        for word in words.iter_mut() {
            while unsafe { !regs.sr().read().txe() } {
                // spin
            }
            unsafe {
                regs.dr().write(|reg| reg.0 = *word as u32);
            }
            while unsafe { !regs.sr().read().rxne() } {
                // spin waiting for inbound to shift in.
            }
            *word = unsafe { regs.dr().read().0 as u16 };
            let sr = unsafe { regs.sr().read() };
            if sr.fre() {
                return Err(Error::Framing);
            }
            if sr.ovr() {
                return Err(Error::Overrun);
            }
            if sr.crcerr() {
                return Err(Error::Crc);
            }
        }

        Ok(words)
    }
}
