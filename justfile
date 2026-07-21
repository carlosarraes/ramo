build:
    cargo build --release
    mkdir -p ~/.local/bin
    cp target/release/ramo ~/.local/bin/
    @echo "Installed ramo to ~/.local/bin/"

install-pi:
    ~/.local/bin/ramo install pi
    @echo "Pi extension installed"

install: build install-pi
