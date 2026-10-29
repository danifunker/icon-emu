ROM ?= $(HOME)/repos/icon_icon2_research/firmware/ICON1/icon1_rom_combined.bin
CARGO := PATH=$(HOME)/.cargo/bin:$$PATH cargo
WASM := target/wasm32-unknown-unknown/release/icon_web.wasm

.PHONY: web serve test cli

web:
	$(CARGO) build --release -p icon-web --target wasm32-unknown-unknown
	$(HOME)/.cargo/bin/wasm-bindgen --target web --no-typescript --out-dir web/pkg $(WASM)
	mkdir -p web/rom && cp $(ROM) web/rom/icon1_rom_combined.bin
	scripts/web-version.sh
	if [ -f disks/hd64-factory.bin ]; then cp disks/hd64-factory.bin web/rom/; fi

serve: web
	cd web && python3 -m http.server 8086

cli:
	$(CARGO) build --release -p icon-cli

test:
	$(CARGO) test --release -p icon-core
