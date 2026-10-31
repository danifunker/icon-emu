ROM ?= $(HOME)/repos/icon_icon2_research/firmware/ICON1/icon1_rom_combined.bin
# The Rel 2.11 ROM donated by vintagecomputer.ca, for the "Download ROM" button.
REL211 ?= $(HOME)/repos/icon_icon2_research/firmware/ICON1/icon1_rom_rel2.11_combined.bin
REL211_URL := https://github.com/danifunker/icon-emu/releases/download/rom-rel2.11/icon1_rom_rel2.11_combined.bin
CARGO := PATH=$(HOME)/.cargo/bin:$$PATH cargo
WASM := target/wasm32-unknown-unknown/release/icon_web.wasm

.PHONY: web serve test cli

web:
	$(CARGO) build --release -p icon-web --target wasm32-unknown-unknown
	$(HOME)/.cargo/bin/wasm-bindgen --target web --no-typescript --out-dir web/pkg $(WASM)
	mkdir -p web/rom && cp $(ROM) web/rom/icon1_rom_combined.bin
	if [ -f $(REL211) ]; then cp $(REL211) web/rom/; else curl -fsSL -o web/rom/icon1_rom_rel2.11_combined.bin $(REL211_URL); fi
	scripts/web-version.sh
	if [ -f disks/hd64-factory.bin ]; then cp disks/hd64-factory.bin web/rom/; fi

serve: web
	cd web && python3 -m http.server 8086

cli:
	$(CARGO) build --release -p icon-cli

test:
	$(CARGO) test --release -p icon-core -p icon-printer
