default:
    @just --list

build:
    cargo build --release

install: build
    ./install.sh

clean:
    cargo clean
