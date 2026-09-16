PREFIX ?= $(HOME)/.local

.PHONY: build test install uninstall

build:
	cargo build --release

test:
	cargo test

install: build
	install -Dm755 target/release/inkstone "$(PREFIX)/bin/inkstone"
	install -Dm644 data/dev.inkstone.Inkstone.desktop \
		"$(PREFIX)/share/applications/dev.inkstone.Inkstone.desktop"
	install -Dm644 data/icons/hicolor/index.theme \
		"$(PREFIX)/share/inkstone/icons/hicolor/index.theme"
	install -d "$(PREFIX)/share/icons/hicolor/scalable/actions"
	install -m644 data/icons/hicolor/scalable/actions/*.svg \
		"$(PREFIX)/share/icons/hicolor/scalable/actions/"
	install -d "$(PREFIX)/share/inkstone/icons/hicolor/scalable/actions"
	install -m644 data/icons/hicolor/scalable/actions/*.svg \
		"$(PREFIX)/share/inkstone/icons/hicolor/scalable/actions/"

uninstall:
	rm -f "$(PREFIX)/bin/inkstone"
	rm -f "$(PREFIX)/share/applications/dev.inkstone.Inkstone.desktop"
	rm -f "$(PREFIX)/share/icons/hicolor/scalable/actions/inkstone-*-symbolic.svg"
	rm -rf "$(PREFIX)/share/inkstone"
