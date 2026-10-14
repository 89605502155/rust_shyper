# trosna-shyper

[English version](README.md)

**trosna-shyper** — встраиваемый гипервизор первого типа (bare-metal) на Rust
для AArch64 и RISC-V. Основан на
[Rust-Shyper](https://github.com/openeuler-mirror/rust_shyper) (Бэйханский
университет, Huawei).

По сравнению с Rust-Shyper, trosna-shyper:

- собирается **стабильным Rust 1.99**; апстриму был нужен
  nightly-2023-11-09;
- **не содержит кода на C и FFI**. Ему нужен **только инструментарий Rust**:
  без кросс-gcc, binutils и libclang;
- работает через обычные `cargo build` / `cargo run`;
- содержит исправления ряда ошибок;
- поставляется с готовым окружением Fedora 42 для управляющей и гостевой ВМ.

## Быстрый старт

На Linux-хосте с [rustup](https://rustup.rs), `qemu-system-aarch64` и docker:

```sh
./tools/mkrootfs.sh      # диск MVM vm0.img: Fedora 42 (AArch64) + гостевая ВМ
cargo run --release      # собрать и загрузить гипервизор в QEMU
```

Гипервизор запускает управляющую ВМ (MVM). В приглашении `[mvm ~]#`:

```sh
/root/start-gvm.sh             # настроить и запустить гостевую ВМ 1
/root/shyper vm console        # её консоль (Ctrl-] — назад)
/root/shyper vm list
```

Выход из QEMU: `Ctrl-A`, `X`.

## Документация

- [Руководство по эксплуатации (русский)](doc/ru/guide.md): сборка, запуск,
  гостевые ВМ, конфигурация, CLI, проверки, решение проблем
- [Operations guide (English)](doc/en/guide.md)
- [Архитектурная модель](doc/architecture/README.md), восстановлена и проверена `archi`
- [Документация Rust-Shyper](doc/upstream/README.md), сохранена из апстрима

## Платформы

| Платформа | Состояние |
|-----------|-----------|
| QEMU `virt`, AArch64 (GICv2/GICv3) | работает: MVM и гостевые ВМ |
| NVIDIA Jetson TX2, Raspberry Pi 4, Firefly ROC-RK3588S-PC | собираются и проходят проверки (на железе в рамках проекта не запускались) |
| QEMU `virt`, RISC-V 64 (PLIC/AIA) | собирается и проходит проверки |

## Проверки

```sh
./tools/check.sh    # rustfmt, clippy для всех плат, CLI, модульные тесты
```

## Лицензия

trosna-shyper распространяется по лицензии [Mulan PSL v2](LICENSE), как и
Rust-Shyper. Код работы с device tree в `deps/libfdt-rs`, портированный из
libfdt, используется по лицензии BSD 2-Clause, см.
[её LICENSE](deps/libfdt-rs/LICENSE). Происхождение кода описано в
[NOTICE](NOTICE).
