# Sharepaste — top-level convenience targets.
# macOS desktop app: build via Tauri, install to /Applications.

DESKTOP_DIR := clients/desktop
UI_DIR      := $(DESKTOP_DIR)/ui
TAURI_DIR   := $(DESKTOP_DIR)/src-tauri
BUNDLE_DIR  := $(TAURI_DIR)/target/release/bundle
APP_NAME    := sharepaste.app
APP_SRC     := $(BUNDLE_DIR)/macos/$(APP_NAME)
APP_DEST    := /Applications/$(APP_NAME)
DMG_DIR     := $(BUNDLE_DIR)/dmg

.PHONY: help desktop-deps desktop-build desktop-install desktop-reinstall desktop-uninstall desktop-open desktop-dmg desktop-clean check-macos

help:
	@echo "Targets:"
	@echo "  desktop-deps        Install npm deps for desktop + ui"
	@echo "  desktop-build       Build the macOS .app and .dmg via tauri"
	@echo "  desktop-install     Copy the built .app to /Applications"
	@echo "  desktop-reinstall   Build + install (replaces existing copy)"
	@echo "  desktop-uninstall   Remove /Applications/$(APP_NAME)"
	@echo "  desktop-open        Open the bundled .app from the build tree"
	@echo "  desktop-dmg         Open the .dmg in Finder"
	@echo "  desktop-clean       cargo clean + remove ui/dist"

check-macos:
	@if [ "$$(uname -s)" != "Darwin" ]; then \
	  echo "macOS only target — current platform: $$(uname -s)"; exit 1; \
	fi

desktop-deps: check-macos
	@if [ ! -d "$(DESKTOP_DIR)/node_modules" ]; then \
	  echo ">> npm ci ($(DESKTOP_DIR))"; \
	  npm --prefix $(DESKTOP_DIR) ci; \
	fi
	@if [ ! -d "$(UI_DIR)/node_modules" ]; then \
	  echo ">> npm ci ($(UI_DIR))"; \
	  npm --prefix $(UI_DIR) ci; \
	fi

desktop-build: desktop-deps
	npm --prefix $(DESKTOP_DIR) run build
	@test -d "$(APP_SRC)" || { echo "build did not produce $(APP_SRC)"; exit 1; }
	@echo ">> built: $(APP_SRC)"

desktop-install: check-macos
	@test -d "$(APP_SRC)" || { echo "no build at $(APP_SRC) — run 'make desktop-build' first"; exit 1; }
	@if [ -d "$(APP_DEST)" ]; then \
	  echo ">> removing existing $(APP_DEST)"; \
	  rm -rf "$(APP_DEST)"; \
	fi
	@echo ">> copying $(APP_SRC) -> $(APP_DEST)"
	cp -R "$(APP_SRC)" "$(APP_DEST)"
	@xattr -dr com.apple.quarantine "$(APP_DEST)" 2>/dev/null || true
	@echo ">> installed: $(APP_DEST)"

desktop-reinstall: desktop-build desktop-install

desktop-uninstall: check-macos
	@if [ -d "$(APP_DEST)" ]; then \
	  rm -rf "$(APP_DEST)"; echo ">> removed $(APP_DEST)"; \
	else \
	  echo ">> nothing to remove at $(APP_DEST)"; \
	fi

desktop-open: check-macos
	@test -d "$(APP_SRC)" || { echo "no build at $(APP_SRC) — run 'make desktop-build' first"; exit 1; }
	open "$(APP_SRC)"

desktop-dmg: check-macos
	@dmg=$$(ls -1 $(DMG_DIR)/*.dmg 2>/dev/null | head -n1); \
	if [ -z "$$dmg" ]; then echo "no .dmg in $(DMG_DIR) — run 'make desktop-build' first"; exit 1; fi; \
	echo ">> opening $$dmg"; open "$$dmg"

desktop-clean: check-macos
	-cd $(TAURI_DIR) && cargo clean
	-rm -rf $(UI_DIR)/dist
