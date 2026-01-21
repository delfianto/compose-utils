default:
    @just --list

build:
    cargo build --release

install: build
    python3 install/setup.py install

uninstall:
    python3 install/setup.py uninstall

clean:
    cargo clean
