# Changelog / История изменений

## Unreleased / Не выпущено

- Desktop in a QEMU window: `GUI=1 cargo run --release`, emulated PL111
  shown through QEMU ramfb, USB keyboard and tablet passed through on PCIe,
  `mvm-desktop.sh` installs and starts Xfce in the MVM.
  Рабочий стол в окне QEMU: `GUI=1 cargo run --release`, эмулируемый PL111
  через QEMU ramfb, USB-клавиатура и планшет через PCIe, `mvm-desktop.sh`
  ставит и запускает Xfce в MVM.
- QEMU: the PL031 RTC is passed through to the MVM, which had no time
  (1970), so TLS rejected every certificate.
  QEMU: RTC PL031 проброшен в MVM — без часов (1970 год) TLS отвергал все
  сертификаты.
- CLI `system seed-rng`, run by the MVM init: the CRNG of the MVM kernel was
  never initialized, so `getrandom()` and TLS (curl, dnf) blocked.
  CLI `system seed-rng` в init MVM: CRNG ядра MVM не инициализировался, и
  `getrandom()` и TLS (curl, dnf) блокировались.
- `mkrootfs.sh`: the MVM init is `/usr/libexec/trosna-init`, linked from
  `/sbin/init` (packages bringing systemd replace the link).
  `mkrootfs.sh`: init MVM — `/usr/libexec/trosna-init`, `/sbin/init` —
  ссылка на него (пакеты с systemd заменяют ссылку).

## v0.0.1 — 2026-10-14

First release of trosna-shyper, based on Rust-Shyper commit `62a2053`.
Первый релиз trosna-shyper на основе коммита Rust-Shyper `62a2053`.

### Highlights / Главное

- Builds again, on **stable Rust 1.99**, with the Rust toolchain only (no C
  compiler, cross binutils, libclang). `cargo build` / `cargo run` boot it in
  QEMU.
  Снова собирается, **стабильным Rust 1.99**, только инструментарием Rust
  (без компилятора C, кросс-binutils, libclang); `cargo build` / `cargo run`
  запускают его в QEMU.
- No C code and no FFI: libfdt ported to Rust (`deps/libfdt-rs`), the CLI uses
  `rustix` instead of libc / libfdisk.
  Нет кода на C и FFI: libfdt портирован на Rust, CLI использует `rustix`
  вместо libc / libfdisk.
- Ready environment: `tools/mkrootfs.sh` builds an MVM disk with Fedora 42
  (AArch64) and a guest VM; `start-gvm.sh` boots the guest and attaches to it.
  Готовое окружение: `tools/mkrootfs.sh` собирает диск MVM с Fedora 42
  (AArch64) и гостевой ВМ; `start-gvm.sh` запускает гостя и подключается к
  нему.
- CLI: `vm console`, `vm delconfig`, `system insmod`.
- Documentation in English and Russian (`doc/en`, `doc/ru`), architecture
  model checked with `archi` (`doc/architecture`), `tools/check.sh`.
  Документация на английском и русском, архитектурная модель, проверенная
  `archi`, `tools/check.sh`.

### Fixed / Исправлено

- The MVM randomly stopped during boot under QEMU (about one boot in 8):
  QEMU's GICv2 model does not re-evaluate the physical CPU interface after a
  virtual deactivation; the hypervisor tick is kept on the QEMU board.
  MVM случайно останавливалась при загрузке в QEMU (примерно каждая 8-я):
  модель GICv2 в QEMU не пересчитывает физический CPU-интерфейс после
  виртуальной деактивации; на плате QEMU тик гипервизора оставлен включённым.
- Race in the asynchronous task executor: a guest disk request handled on two
  cores panicked the hypervisor (`there is no ipi task`); IPI tasks now own
  their request and are taken off the queue atomically.
  Гонка в исполнителе асинхронных задач: запрос гостя к диску на двух ядрах
  ронял гипервизор; IPI-задачи теперь владеют своим запросом и извлекаются из
  очереди атомарно.
- Removing a VM with a pending GIC maintenance request panicked the hypervisor.
  Удаление ВМ с ожидающим maintenance-запросом GIC роняло гипервизор.
- Unimplemented hypercalls (`todo!()`) and non-mediated disk requests panicked
  the hypervisor; they return an error now.
  Нереализованные гипервызовы и запросы к неопосредованному диску роняли
  гипервизор; теперь возвращают ошибку.
- Removing a VM's tasks kept exactly the tasks to remove.
  Удаление задач ВМ оставляло ровно те задачи, которые надо было удалить.
- CLI: `sys update` sent the reboot hypercall; images larger than 32 MiB were
  uploaded past the buffer; `"-"` was taken as a device tree file name;
  undefined behavior (dangling pointer, unterminated strings, write to an
  immutable value).
  CLI: `sys update` отправлял гипервызов перезагрузки; образы больше 32 МиБ
  загружались за пределы буфера; `"-"` считался именем файла device tree;
  неопределённое поведение.
- Null reference at the hypervisor entry; uninitialized buffer in libfdt-rs.
  Нулевая ссылка на входе гипервизора; неинициализированный буфер в libfdt-rs.

### Known issues / Известные проблемы

- Re-attaching to a guest console after the guest printed output while it was
  detached may echo that output into the guest shell (`bash: ...: command not
  found` lines). `start-gvm.sh` attaches before the guest boots to avoid it.
  Повторное подключение к консоли гостя после того, как гость что-то
  напечатал без подключённой консоли, может вернуть этот вывод в shell гостя
  (строки `bash: ...: command not found`). `start-gvm.sh` подключается до
  запуска гостя, чтобы этого избежать.
- Not implemented (the hypercalls return an error): VM shutdown without
  removal (use `vm remove`), VM state query, hypervisor live update, VM
  migration, non-mediated virtio disks, initrd upload by the CLI.
  Не реализовано (гипервызовы возвращают ошибку): остановка ВМ без удаления
  (используйте `vm remove`), запрос состояния ВМ, горячее обновление,
  миграция ВМ, неопосредованные virtio-диски, загрузка initrd через CLI.
- `tools/shyper.ko` is a prebuilt module for Linux 4.9.140 without sources; an
  MVM with another kernel needs a matching module.
  `tools/shyper.ko` — готовый модуль для Linux 4.9.140 без исходников; для
  MVM с другим ядром нужен соответствующий модуль.
- Run only on QEMU `virt` AArch64 with GICv2. Jetson TX2, Raspberry Pi 4,
  RK3588, QEMU GICv3 and RISC-V build and pass the checks but were not run;
  there is no RISC-V MVM disk script.
  Запускалось только на QEMU `virt` AArch64 с GICv2. Jetson TX2,
  Raspberry Pi 4, RK3588, QEMU GICv3 и RISC-V собираются и проходят
  проверки, но не запускались; скрипта диска MVM для RISC-V нет.
- Guests must be built for the hypervisor architecture (AArch64 or RISC-V);
  x86 systems cannot be guests.
  Гостевые ОС должны быть собраны под архитектуру гипервизора (AArch64 или
  RISC-V); системы для x86 гостями быть не могут.
