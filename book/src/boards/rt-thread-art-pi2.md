# RT-Thread ART-Pi2

This initial configuration supports the STM32H7R7L8 Cortex-M7, the board's
ST-LINK/V2.1 debug interface, and the user LEDs and button. It uses the default
64 MHz internal HSI clock and direct SMPS supply for VCORE.

## Blinky

```sh
laze -C examples/blinky build -b rt-thread-art-pi2
laze -C examples/blinky build -b rt-thread-art-pi2 run
```

Connect the board's ST-LINK USB port. The existing blinky example toggles LED1
(red, PO1, active high) every 500 ms. LED2 is blue (PO5, active low); the user
button is PC13 (active low).

The image runs from the 64 KiB internal Flash at `0x08000000`, with 456 KiB
of contiguous AXI SRAM at `0x24000000`. Programming it replaces the internal
bootloader if one is installed. Back up that Flash before the first download:

```sh
probe-rs read --chip STM32H7R7L8 b8 0x08000000 65536 \
  --output art-pi2-original-internal-flash.bin --format binary
```

External HyperFlash, HyperRAM, USB, networking, and other peripherals are not
configured by this initial board support.

## Validation

Tested on an ART-Pi2 using probe-rs 0.32.0 and the onboard ST-LINK/V2.1:

- Built the unchanged `examples/blinky` in the release profile.
- Downloaded the ELF, verified Flash contents, and reset the board.
- Sampled GPIOO ODR at `0x58023814`; bit 1 repeatedly alternated between 0 and 1,
  consistent with the example's 500 ms delay. This is an electrical register
  check; visual LED behavior was not independently observed.

## References

- [RT-Thread board SDK](https://github.com/RT-Thread-Studio/sdk-bsp-stm32h7r-realthread-artpi2)
- [Zephyr board sources and pin mapping](https://github.com/zephyrproject-rtos/zephyr/blob/main/boards/ruiside/art_pi2/art_pi2.dts)
