//! Complete the two TODOs in this file.

use embassy_stm32::{
    Peripherals, can::Can,
    usart::{BufferedUart, Config as UartConfig, InterruptHandler, Parity, StopBits},
    gpio::{Flex, Level, Output, Speed, Pull},
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use static_cell::StaticCell;

use crate::host_protocol::HostFrame;
use crate::swd::Swd;

embassy_stm32::bind_interrupts!(struct Irqs {
    UART7 => InterruptHandler<embassy_stm32::peripherals::UART7>;
});

static TX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
static RX_BUF: StaticCell<[u8; 256]> = StaticCell::new();

// ---- 方向 1：两个有界队列 ----
pub type HostBridgeChannel = Channel<CriticalSectionRawMutex, HostFrame, 8>;

/// CAN 接收 -> UART 转发
pub static CAN_TO_HOST: HostBridgeChannel = Channel::new();

/// UART 接收 -> CAN 发送
pub static HOST_TO_CAN: HostBridgeChannel = Channel::new();

pub struct Ports {
    pub can: Can<'static>,
    pub uart: Option<BufferedUart<'static, embassy_stm32::peripherals::UART7>>,
}

pub fn configure(p: Peripherals) -> (crate::board::Usb, Ports, Option<Swd>) {
    let usb = crate::board::usb(p.USB_OTG_HS, p.PA12, p.PA11);

    let swclk = Output::new(p.PA00, Level::Low, Speed::Low);

    let mut swdio = Flex::new(p.PA02);
    swdio.set_as_input(Pull::Up);

    let swd = Some(Swd::new(swclk, swdio));

    let tx_buf = TX_BUF.init([0; 256]);
    let rx_buf = RX_BUF.init([0; 256]);

    let mut config = UartConfig::default();
    config.baudrate = 115200;
    config.parity = Parity::ParityNone;
    config.stop_bits = StopBits::STOP1;

    let uart = BufferedUart::new(
        p.UART7,
        p.PE07,
        p.PE08,
        Irqs,
        tx_buf,
        rx_buf,
        config,
    ).unwrap();

    let uart = Some(uart);

    let can = crate::board::can1(p.FDCAN1, p.PD0, p.PD1);
    (usb, Ports { can, uart }, swd)
}
