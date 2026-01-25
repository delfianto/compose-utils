# Maintainer: Dwi Elfianto <dwi@elfianto.com>

pkgname=compose-utils
pkgver=0.1.0
pkgrel=1
pkgdesc="Systemd integration for Docker Compose projects"
arch=('x86_64')
url="https://github.com/delfianto/compose-utils"
license=('MIT')
depends=('docker' 'systemd')
makedepends=('cargo' 'git')
source=("git+file://$(pwd)") # Assumes you build from local git
sha256sums=('SKIP')

prepare() {
    cd "$srcdir/$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    cargo fetch --locked --target "$CARCH-unknown-linux-gnu"
}

build() {
    cd "$srcdir/$pkgname"
    export RUSTUP_TOOLCHAIN=stable
    export CARGO_TARGET_DIR=target
    cargo build --frozen --release --all-features
}

package() {
    cd "$srcdir/$pkgname"

    # Install binary
    install -Dm755 "target/release/compose" "$pkgdir/usr/bin/compose"

    # Install Systemd Template (Create this folder structure in your source first)
    install -Dm644 "systemd/compose@.service" "$pkgdir/usr/lib/systemd/system/compose@.service"

    # Install License
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}
