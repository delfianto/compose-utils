default:
    @just --list

build: check
    cargo build --release

check:
    cargo fmt
    cargo clippy -- -D warnings
    cargo test
    cargo build --release
    
clean:
    cargo clean

install *args: build
    python3 install/setup.py install {{ args }}

uninstall *args:
    python3 install/setup.py uninstall {{ args }}

reinstall: build
    python3 install/setup.py reinstall
