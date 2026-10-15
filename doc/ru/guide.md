# Руководство по эксплуатации trosna-shyper

[English version](../en/guide.md)

Это руководство описывает, как собрать trosna-shyper, запустить его в QEMU,
работать с управляющей ВМ и запускать гостевые виртуальные машины. Также здесь
описаны конфигурация, утилита командной строки, проверки и решение проблем.

Содержание:

1. [Что такое trosna-shyper](#1-что-такое-trosna-shyper)
2. [Требования к хосту](#2-требования-к-хосту)
3. [Получение исходников](#3-получение-исходников)
4. [Сборка](#4-сборка)
5. [Диск MVM](#5-диск-mvm)
6. [Запуск в QEMU](#6-запуск-в-qemu)
7. [Работа в MVM](#7-работа-в-mvm)
8. [Гостевые ВМ](#8-гостевые-вм)
9. [Справочник по конфигурации ВМ](#9-справочник-по-конфигурации-вм)
10. [Справочник по утилите `shyper`](#10-справочник-по-утилите-shyper)
11. [Другие платы и RISC-V](#11-другие-платы-и-risc-v)
12. [Проверки и тесты](#12-проверки-и-тесты)
13. [Отладка](#13-отладка)
14. [Проблемы и известные ошибки](#14-проблемы-и-известные-ошибки)
15. [Отличия от Rust-Shyper](#15-отличия-от-rust-shyper)
16. [Структура репозитория](#16-структура-репозитория)

## 1. Что такое trosna-shyper

trosna-shyper — **гипервизор первого типа** (bare-metal), написанный на Rust.
Он работает прямо на оборудовании: в режиме EL2 на AArch64 или в HS-режиме на
RISC-V. Под ним нет хостовой операционной системы. Проект основан на
[Rust-Shyper](https://github.com/openeuler-mirror/rust_shyper) от Бэйханского
университета и Huawei и собирается только стабильным Rust.

Гипервизор запускает виртуальные машины двух видов:

- **MVM (management VM, управляющая ВМ)**, ВМ 0. Стартует автоматически
  вместе с гипервизором; её ядро Linux встроено в образ гипервизора. Ей
  принадлежит большая часть физических устройств. Другими ВМ она управляет
  утилитой `shyper` через модуль ядра `shyper.ko`.
- **GVM (гостевые ВМ)**, ВМ 1 и далее. MVM их настраивает и запускает.
  Каждая GVM получает собственные физические ядра процессора и память,
  эмулируемый контроллер прерываний и virtio-устройства:
  - блочное устройство, которое обслуживает MVM («опосредованный» диск);
  - сетевое устройство;
  - консоль.

  Можно также пробросить ей физические устройства (passthrough).

Изоляция обеспечивается аппаратной виртуализацией: памяти — двухступенчатой
трансляцией адресов (stage-2), прерываний — виртуальным GIC/PLIC.

Гостевая ОС должна быть собрана под ту же архитектуру, что и гипервизор
(AArch64 или RISC-V). Проверенная гостевая ОС — Linux. В примерах ниже MVM
и GVM используют пользовательское окружение Fedora 42 для AArch64.

## 2. Требования к хосту

Хост — машина, на которой идёт сборка и запускается QEMU (Linux, x86_64 или
AArch64).

| Что | Зачем | Пример (Ubuntu / Debian) |
|-----|-------|--------------------------|
| [rustup](https://rustup.rs) | инструментарий Rust. Точная версия (Rust 1.99), цели и компоненты ставятся автоматически по файлу `rust-toolchain`. | см. rustup.rs |
| `qemu-system-aarch64` | запуск гипервизора | `apt install qemu-system-arm` |
| `qemu-system-riscv64` (необязательно) | RISC-V | `apt install qemu-system-misc` |
| docker, `mkfs.ext4`, sudo | только для сборки диска MVM (`tools/mkrootfs.sh`) | `apt install docker.io e2fsprogs` |

Компилятор C, кросс-тулчейн, binutils, libclang и `libfdisk` **не нужны**.
Гипервизор, его библиотеки и CLI написаны только на Rust. Компоновка и
преобразование образов выполняются `rust-lld` и `llvm-objcopy` из rustup.

Пути с пробелами и не-ASCII-символами (например, `~/Рабочий стол`)
поддерживаются.

## 3. Получение исходников

```sh
git clone <адрес репозитория> trosna-shyper
cd trosna-shyper
```

Первая же команда cargo установит инструментарий по `rust-toolchain`:
Rust 1.99, цели `aarch64-unknown-none-softfloat`,
`riscv64imac-unknown-none-elf`, `aarch64-unknown-linux-musl`,
`riscv64gc-unknown-linux-musl` и компоненты `rust-src`, `llvm-tools`,
`clippy`, `rustfmt`.

## 4. Сборка

### Гипервизор (QEMU, AArch64)

```sh
cargo build --release
```

Результат — `target/aarch64-unknown-none-softfloat/release/trosna-shyper`,
ELF-образ со встроенным ядром MVM. По умолчанию встраивается
`image/Image_vanilla` (Linux 4.9.140). Другое ядро:
`VM0_IMAGE_PATH=путь/к/Image cargo build --release`.

Плата по умолчанию — `qemu`. Другие платы выбираются фичами, см.
[раздел 11](#11-другие-платы-и-risc-v).

### Через make

Makefile делает то же самое и дополнительно создаёт «сырой» двоичный образ
(`.bin`) для загрузчиков:

```sh
make qemu      # target/aarch64-unknown-none-softfloat/release/trosna-shyper.bin
make run       # собрать и запустить QEMU (8 ГиБ)
make tx2       # / pi4 / rk3588: образы для плат (для шага upload нужны mkimage и TFTP-сервер)
```

### Утилита CLI (`shyper`)

CLI работает внутри MVM. Она кросс-компилируется в статический исполняемый
файл Linux, компоновщик — `rust-lld`:

```sh
cd cli
cargo build --release --target aarch64-unknown-linux-musl
# -> cli/target/aarch64-unknown-linux-musl/release/shyper
```

`tools/mkrootfs.sh` собирает её автоматически.

## 5. Диск MVM

Ядро MVM монтирует корневую файловую систему с `vm0.img` (блочное
virtio-устройство `/dev/vda`). Диск создаётся так:

```sh
./tools/mkrootfs.sh            # создаёт vm0.img (3 ГиБ)
```

Скрипт:

- экспортирует через docker официальный образ
  `registry.fedoraproject.org/fedora:42` для `linux/arm64`. На хосте ничего
  не запускается под arm64; если скрипт сам скачал образ, он его потом
  удаляет.
- ставит небольшой `/sbin/init`: он монтирует `/proc`, `/sys`, `/dev`,
  загружает `shyper.ko` и запускает root-shell на консоли.
- кладёт в `/root`:
  - `shyper` — CLI, собранная из `cli/`;
  - `shyper.ko` — модуль ядра MVM (собран для Linux 4.9.140, взят из
    `tools/`);
  - `gvm/Image` — ядро гостя;
  - `gvm/gvm.img` — корневой диск гостя: Fedora 42 с init, запускающим
    shell на `/dev/hvc0`;
  - `gvm/gvm.json` и `gvm/mediated.json` — конфигурация гостя (из
    `tools/gvm-qemu.json` и `tools/mediated-qemu.json`);
  - `start-gvm.sh` — настраивает и запускает гостя и подключается к его
    консоли.

Переменные: `SIZE` (размер диска MVM, по умолчанию `3G`), `GVM_SIZE` (по
умолчанию `768M`), `GVM_KERNEL` (по умолчанию `image/Image_vanilla`),
`FEDORA_IMAGE`, `SUDO` (например, `SUDO="sudo -A"`).

## 6. Запуск в QEMU

```sh
cargo run --release
```

`cargo run` собирает гипервизор и запускает его через `tools/qemu-run.sh`:

- машина `virt` с включённой виртуализацией (EL2) и GICv2;
- 4 ядра (Cortex-A57) и 4 ГиБ памяти;
- диск `vm0.img`;
- пользовательская сеть: порт 5555 хоста проброшен на порт 22 MVM.

Последовательная консоль выводится прямо в терминал.

- `QEMU_MEM=6g cargo run --release` меняет объём памяти. На слабом хосте
  не давайте больше 8 ГиБ.
- `DISK=other.img cargo run --release` подключает другой диск.
- Дополнительные аргументы передаются QEMU:
  `cargo run --release -- -s -S` (для gdb).

**Выход из QEMU:** `Ctrl-A`, затем `X`.

### Рабочий стол в окне QEMU

```sh
GUI=1 cargo run --release
```

открывает окно QEMU. MVM получает видеопамять (эмулируемый PL111, который
QEMU показывает через `ramfb`) и USB-клавиатуру и планшет QEMU (проброшены
через PCIe, поэтому PCIe остаётся ниже 4 ГиБ:
`highmem-ecam=off,highmem-mmio=off`). Последовательная консоль остаётся в
терминале. `QEMU_DISPLAY=sdl` выбирает другой дисплей QEMU.

В приглашении `[mvm ~]#` один раз (около 1 ГБ через сеть QEMU):

```sh
/root/mvm-desktop.sh install
```

Скрипт ставит Xorg и Xfce через dnf. В Fedora 42 больше нет драйвера Xorg
fbdev, а в ядре MVM нет DRM-драйвера, поэтому скрипт пересобирает из
исходников последний пакет драйвера из Fedora 40 и затем удаляет
компилятор. Дальше каждый раз:

```sh
/root/mvm-desktop.sh
```

запускает Xfce в окне. В MVM нет udev, поэтому скрипт пишет
`/etc/X11/xorg.conf.d/10-trosna.conf` с найденными устройствами.


Что видно при загрузке:

1. `Welcome to qemu trosna-shyper 0.0.1!`, информация о сборке и журнал
   гипервизора (`[I]>[core N, ...]`): инициализация памяти, 4 ядер,
   контроллера прерываний и таймера.
2. `Start booting Monitor VM ...`, затем журнал ядра Linux в MVM. Журнал
   гипервизора продолжает выводиться в ту же консоль цветом, например при
   настройке virtio-устройств.
3. `mvm: Fedora Linux 42 (Container Image), kernel 4.9.140 (aarch64)` и
   приглашение `[mvm ~]#`.

Загрузка занимает примерно 1–2 минуты: QEMU эмулирует процессоры AArch64.

## 7. Работа в MVM

MVM — обычный Linux с root-shell, плюс инструменты гипервизора:

```sh
cat /etc/os-release
ls -l /dev/shyper          # устройство модуля ядра гипервизора
./shyper --help
./shyper vm list           # ВМ 0 "supervisor" — это сама MVM
./shyper vm getvmid
```

`/root/shyper` и `/root/shyper.ko` — инструменты, `/root/gvm` — гость.

## 8. Гостевые ВМ

### Быстрый старт

```sh
/root/start-gvm.sh
```

Скрипт запускает гостевую ВМ 1 и подключает ваш терминал к её консоли: видна
загрузка ядра гостя. Когда вывод затихнет, нажмите Enter. Приглашение
`[gvm ~]#` — это уже гостевая ВМ (имя хоста `fedora-guest`):

```sh
uname -a        # Linux fedora-guest 4.9.140 ... aarch64
cat /etc/os-release
df -h /         # /dev/root — опосредованный диск gvm.img
nproc           # 1: у гостя одно собственное физическое ядро
```

`Ctrl-]` возвращает в MVM, `/root/shyper vm console` подключает снова.

### По шагам

`start-gvm.sh` делает следующее:

```sh
cd /root/gvm
# 1. Демон обслуживает опосредованные диски гостей и запросы гипервизора
#    (работает постоянно).
/root/shyper system daemon mediated.json > daemon.log 2>&1 &
# 2. Передать гипервизору конфигурацию и ядро гостя; новая ВМ получит id 1.
/root/shyper vm config gvm.json
# 3. Подключиться к её консоли и запустить её на выделенном ядре
#    (ядро 2 в gvm.json).
(sleep 1 && /root/shyper vm boot 1) &
exec /root/shyper vm console
```

`/root/shyper vm console` подключается к консоли гостя (`/dev/hvc0` в MVM),
переводя устройство и ваш терминал в raw-режим. Консоль подключается до
запуска гостя намеренно: вывод, который гость печатает, пока консоль никто не
открыл, драйвер MVM придерживает и при открытии может вернуть гостю эхом.

### Управление гостями

```sh
/root/shyper vm list               # ВМ и их состояние
/root/shyper vm reboot 1 [-f]      # перезагрузить гостя
/root/shyper vm remove 1           # остановить и удалить гостя (вместе с конфигурацией)
/root/shyper vm delconfig 1        # удалить конфигурацию, которая не запускалась
```

### Свой гость

1. Скопируйте в MVM ядро Linux (`Image`) для AArch64. В ядро должны быть
   встроены драйверы virtio-mmio: блочного устройства, сети и консоли.
2. Подготовьте корневой диск (любой образ ext4 или блочное устройство) и
   впишите его в `mediated.json`. Первая запись — опосредованный диск 0,
   следующая — диск 1 и т.д.
3. Отредактируйте `gvm.json`:
   - `kernel_filename`;
   - `cmdline` (консоль `hvc0`, корень `/dev/vda`);
   - память;
   - битовую маску ядер;
   - в `cfg_list` устройства `EMU_DEVICE_T_VIRTIO_BLK_MEDIATED`: номер диска
     и его размер в секторах по 512 байт.
4. Если менялся `mediated.json`, перезапустите демон. Затем выполните
   `vm config` и `vm boot`.

## 9. Справочник по конфигурации ВМ

Пример — `tools/gvm-qemu.json`. Числа задаются десятичными или строками в
шестнадцатеричном (`"0x80000000"`) либо двоичном (`"0b0100"`) виде.

| Поле | Значение |
|------|----------|
| `name` | имя ВМ (видно в `vm list`) |
| `type` | `VM_T_LINUX`, `VM_T_BARE_MATEL_APP` или `VM_T_FREERTOS` |
| `cmdline` | командная строка ядра |
| `image.kernel_filename` | файл ядра относительно каталога, где запускается CLI (не более 31 символа) |
| `image.kernel_load_ipa` | гостевой физический адрес ядра |
| `image.kernel_entry_point` | точка входа (справочно, используется адрес загрузки) |
| `image.device_tree_filename` | свой device tree или `"-"`, чтобы его сгенерировал гипервизор |
| `image.device_tree_load_ipa` | адрес device tree |
| `image.ramdisk_filename`, `ramdisk_load_ipa` | initrd: CLI пока его не загружает, указывайте `"-"` и `0` |
| `memory.region[]` | `ipa_start` и `length` каждой области памяти гостя |
| `memory.colors`, `memory.budget` | необязательно: раскраска кэша (`"0-13,15"`) и бюджет пропускной способности памяти |
| `cpu.num`, `cpu.allocate_bitmap`, `cpu.master` | число vCPU, битовая маска физических ядер (ядро 0 занято MVM), загрузочное ядро |
| `emulated_device.emulated_device_list[]` | эмулируемые устройства: `name`, `base_ipa`, `length`, `irq_id`, `cfg_list`, `type` |
| `passthrough_device.passthrough_device_list[]` | физические области, отображаемые в гостя: `base_pa`, `base_ipa`, `length`, `irq_list`, необязательный `smmu_id` |
| `dtb_device.dtb_device_list[]` | узлы, добавляемые в генерируемый device tree (контроллер прерываний и т.п.) |

Типы эмулируемых устройств (`type`) и `cfg_list`, где он важен:

| Тип | Устройство | `cfg_list` |
|-----|------------|------------|
| `EMU_DEVICE_T_GICD`, `EMU_DEVICE_T_GICR` | виртуальный дистрибьютор / редистрибьютор GIC | – |
| `EMU_DEVICE_T_PLIC`, `EMU_DEVICE_T_APLIC` | виртуальный контроллер прерываний (RISC-V) | – |
| `EMU_DEVICE_T_VIRTIO_BLK_MEDIATED` | блочное virtio-устройство, обслуживаемое демоном MVM | `[номер диска, размер в секторах]` |
| `EMU_DEVICE_T_VIRTIO_BLK` | блочное virtio-устройство без бэкенда: не поддерживается, запросы отбрасываются с предупреждением | – |
| `EMU_DEVICE_T_VIRTIO_NET` | сетевое virtio-устройство, коммутацию между ВМ выполняет гипервизор | MAC-адрес, 6 байт |
| `EMU_DEVICE_T_VIRTIO_CONSOLE` | virtio-консоль, соединённая с другой ВМ | `[id ВМ-партнёра, адрес её консоли]` |

Опосредованные диски (`mediated.json`):

```json
{ "mediated": ["/root/gvm/gvm.img", "/dev/sda2"] }
```

Сама MVM настраивается в исходниках гипервизора, `src/config/<плата>_def.rs`
(для QEMU — `src/config/qemu_def.rs`): её память, ядра, устройства и
командная строка ядра.

## 10. Справочник по утилите `shyper`

```
shyper system daemon [mediated.json]   запустить демон (опосредованные диски, загрузка образов)
shyper system insmod [shyper.ko]       загрузить модуль ядра (kmod не нужен)
shyper system seed-rng                 заполнить ГСЧ ядра (делает init MVM)
shyper system reboot [-f]              перезагрузить гипервизор
shyper system shutdown [-f]            выключить гипервизор
shyper system update <образ>           горячее обновление (в trosna-shyper пока не поддерживается)
shyper system test                     тестовый гипервызов
shyper vm config <vm.json>             добавить ВМ, выводит её id
shyper vm boot <id>                    запустить ВМ
shyper vm console [/dev/hvc0]          подключиться к консоли гостя, Ctrl-] — выход
shyper vm list                         список ВМ
shyper vm reboot <id> [-f]             перезагрузить ВМ
shyper vm remove <id>                  остановить и удалить ВМ
shyper vm delconfig <id>               удалить конфигурацию ВМ
shyper vm getvmid                      id ВМ, в которой выполняется команда
```

CLI общается с гипервизором через `/dev/shyper` с помощью ioctl, которые
модуль ядра превращает в гипервызовы. Функции библиотеки C напрямую не
вызываются, только системные вызовы Linux через `rustix`.

## 11. Другие платы и RISC-V

Платы выбираются фичами cargo; соберите без фичи `qemu` по умолчанию:

```sh
cargo build --release --no-default-features --features tx2      # NVIDIA Jetson TX2
cargo build --release --no-default-features --features pi4      # Raspberry Pi 4
cargo build --release --no-default-features --features rk3588   # Firefly ROC-RK3588S-PC
cargo build --release --no-default-features --features qemu,gicv3
```

`make tx2` / `make pi4` / `make rk3588` дополнительно задают адрес загрузки
и образ MVM для платы и делают образ для U-Boot (цель `upload`). Эти платы
собираются и проходят проверки, но в рамках проекта запускался только QEMU.

RISC-V (QEMU, контроллер прерываний PLIC или AIA):

```sh
cargo build --release --target riscv64imac-unknown-none-elf --no-default-features --features qemu,plic
ARCH=riscv64 make qemu          # то же через make
```

Для RISC-V ядро MVM — `image/Image-6.10-rc1`, QEMU нужно 8 ГиБ
(`cargo run --target riscv64imac-unknown-none-elf ...`). Диск MVM и модуль
ядра для RISC-V описаны в
[руководстве апстрима](../upstream/RISC-V64_User_Guide.md).

## 12. Проверки и тесты

```sh
./tools/check.sh
```

Запускает:

- `cargo fmt --check`;
- `cargo clippy` для всех 9 комбинаций плат и фич гипервизора;
- `cargo clippy` для CLI под AArch64 и RISC-V;
- модульные тесты крейта `fdt` (правка device tree и оверлеи).

Завершается с ошибкой при любом предупреждении.

Отдельные проверки:

```sh
cargo fmt --all --check
cargo clippy --release
cargo test --manifest-path deps/libfdt-rs/Cargo.toml --target x86_64-unknown-linux-gnu
```

## 13. Отладка

- Журнал гипервизора — цветные строки `[E]/[W]/[I]` в последовательной
  консоли.
- Монитор QEMU: при `-nographic` сочетание `Ctrl-A`, затем `C` переключает
  между консолью и монитором. `info registers -a` показывает все ядра: EL1 —
  работает гость, EL2 — гипервизор.
- gdb: `cargo run --release -- -s -S` ждёт отладчик на порту 1234, затем
  `gdb-multiarch -x gdb/aarch64.gdb`. То же через make: `make debug` /
  `make gdb`.

## 14. Проблемы и известные ошибки

| Симптом | Причина / решение |
|---------|-------------------|
| `no valid device tree from the boot loader` | QEMU передаёт device tree только «сырому» образу. Используйте `cargo run`, `make run` или `.bin`, а не ELF напрямую. |
| `VFS: Unable to mount root fs` в MVM | нет `vm0.img` или это не образ диска: выполните `tools/mkrootfs.sh`. |
| Консоль гостя повторяет собственный вывод | устройство консоли использовали без raw-режима: подключайтесь через `shyper vm console`. |
| `vm config` падает с `failed to copy kernel image` | неверный `kernel_filename` или нет свободных huge pages (командная строка MVM резервирует 4 × 32 МиБ). |
| Docker: permission denied в `mkrootfs.sh` | пользователь не в группе `docker`. Запустите скрипт с правами sudo или добавьте пользователя в группу. |
| Очень медленная загрузка | QEMU эмулирует процессоры (TCG). Закройте другие тяжёлые программы. |

### Примечание: прерывания гостей в QEMU

В модели GICv2 в QEMU (проверено до версии 10.2) есть ошибка. Когда гость
деактивирует аппаратно-связанное прерывание через виртуальный CPU-интерфейс,
QEMU не пересчитывает физический CPU-интерфейс. Если прерывание таймера гостя
в этот момент уже снова ожидает обработки, сигнал о нём не подаётся до любого
другого события в GIC. В Rust-Shyper из-за этого примерно каждая 8-я загрузка
MVM навсегда останавливалась после `NET: Registered protocol family 1`.

trosna-shyper обходит это только для платы `qemu`: таймер гипервизора тикает
и на ядрах с единственным vCPU (раз в 10 мс), и каждый тик заставляет QEMU
пересчитать состояние. На реальных платах таймер для одного vCPU, как и
раньше, выключен. Замер: 0 зависаний на 90 загрузок с обходом, около 12% без
него.

## 15. Отличия от Rust-Shyper

По сравнению с коммитом апстрима `62a2053`:

- **Снова собирается.** Пути с пробелами обрабатываются. Сборка больше не
  зависит от заголовков glibc хоста.
- **Стабильный Rust 1.99** вместо nightly-2023-11-09:
  - не используется ни одного `#![feature]`;
  - встроенные цели заменили собственные target JSON и `-Z build-std`;
  - naked-функции написаны через `naked_asm!`;
  - интринсики H-расширения RISC-V заменены вспомогательными функциями на
    inline asm.
- **Нет C и FFI:**
  - C-библиотека libfdt (bindgen + cc) портирована в крейт на Rust
    `deps/libfdt-rs`;
  - `vm-fdt` берётся с crates.io вместо старого git-форка;
  - CLI использует `rustix` вместо `libc` и больше не требует `libfdisk`.
- **Только инструментарий Rust.** `rust-lld` и llvm-tools заменили GNU
  кросс-тулчейн. Образ MVM встраивается через `.incbin`.
  `cargo build`/`cargo run` работают без Makefile.
- **Исправленные ошибки:**
  - удаление задач ВМ оставляло ровно те задачи, которые надо было удалить;
  - `sys update` отправлял гипервызов перезагрузки вместо обновления;
  - образы больше 32 МиБ загружались за пределы буфера;
  - device tree `"-"` воспринимался как имя файла;
  - неопределённое поведение в CLI: висячий указатель, строки без
    завершающего нуля, запись в неизменяемую переменную;
  - нулевая ссылка на входе гипервизора;
  - гонка в исполнителе асинхронных задач: гипервизор падал, когда запрос
    гостя к диску обрабатывался на двух ядрах одновременно;
  - нереализованные гипервызовы и запросы к неопосредованному диску
    роняли гипервизор паникой, теперь они возвращают ошибку;
  - libfdt оставлял неинициализированные данные в буферах;
  - под QEMU MVM случайно останавливалась при загрузке (терялось
    прерывание таймера гостя, см. раздел 14).
- **Новое:**
  - `vm delconfig`, `vm console`, `system insmod`, `system seed-rng` в CLI;
  - диск MVM с готовым гостем (`tools/mkrootfs.sh`);
  - `tools/check.sh`.
- **Чистый код:** `clippy` и `rustfmt` не выдают предупреждений ни для
  одной платы.

## 16. Структура репозитория

```
src/                гипервизор (arch/, kernel/, device/, vmm/, board/, config/, ...)
src/main.rs         загрузочный образ (cargo build / cargo run)
deps/libfdt-rs/     правка device tree на Rust (порт libfdt)
cli/                shyper — утилита управления для MVM
linkers/            скрипты компоновщика
image/              образы ядер MVM
tools/              mkrootfs.sh, qemu-run.sh, check.sh, конфигурация гостя, shyper.ko
doc/                эта документация; doc/upstream — документация Rust-Shyper
```
