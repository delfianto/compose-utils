default:
    @just --list

build:
    cargo build --release

install *args: build
    python3 install/setup.py install {{ args }}

uninstall *args:
    python3 install/setup.py uninstall {{ args }}

reinstall:
    python3 install/setup.py reinstall

clean:
    cargo clean
