# RT-Thread ART-Pi2

This initial configuration supports the STM32H7R7L8 Cortex-M7, the board's
ST-LINK/V2.1 debug interface, and the user LED and button. It uses the default
64 MHz internal HSI clock and direct SMPS supply for VCORE.

## Blinky

```sh
laze -C examples/blinky build -b rt-thread-art-pi2
laze -C examples/blinky build -b rt-thread-art-pi2 run
```

Connect the board's ST-LINK USB port. The user LED is a single red/blue package:
the red channel is `led0`, PO1 (active high), and the blue channel is `led1`, PO5
(active low). The unchanged Ariel OS blinky example toggles the red channel
every 500 ms. The user button is PC13 (active low).

Both channels start off. This is configured in the board YAML:

```yaml
ariel:
  swi: WWDG
  global_env:
    CONFIG_GPIO_OUTPUTS: PO1:low,PO5:high
```

`CONFIG_GPIO_OUTPUTS` sets STM32 GPIO output levels after HAL initialization,
before application startup. It accepts comma-separated `PIN:low` or `PIN:high`
entries. The initialization borrows each peripheral, so applications can still
take ownership of both LED pins. An unused channel retains its initial output
level. The `active` field describes polarity; it does not initialize a pin.

The core-board schematic identifies this dual LED as D1. The base board has
another power indicator and a dual-color ST-LINK indicator. Those indicators
are separate from the user LED. `LED_OB_EN` is pulled high on the core board;
it is brought out to the board connector for optional external control.

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

- Built the unchanged `examples/blinky` and `examples/minimal` in the release
  profile, downloaded the images, and verified Flash contents.
- With minimal running, GPIOO MODER was `0x7f7`, and both IDR and ODR were
  `0x20`: PO1 and PO5 were outputs, red was low, and blue was high. This checks
  the YAML defaults without application code using the LED pins.
- Restored unchanged blinky. Both IDR and ODR alternated between `0x20` and
  `0x22`, with blue held off while red toggled. Visual confirmation of this
  final image is pending; register activity alone does not prove visible light.
- A separate diagnostic driving both channels was visually confirmed to
  alternate red and blue. It is not part of the blinky example.
- Built minimal for `st-nucleo-h753zi` to check the default initialization path
  when `CONFIG_GPIO_OUTPUTS` is unset.

## References

- [RT-Thread board SDK](https://github.com/RT-Thread-Studio/sdk-bsp-stm32h7r-realthread-artpi2)
- [Core-board schematic, sheet 2](https://github.com/RT-Thread-Studio/sdk-bsp-stm32h7r-realthread-artpi2/blob/master/documents/board/STM32H7/ART-Pi2_Core_A_SCH_V1.0.pdf)
- [Zephyr board sources and pin mapping](https://github.com/zephyrproject-rtos/zephyr/blob/main/boards/ruiside/art_pi2/art_pi2.dts)
