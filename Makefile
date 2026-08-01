# Sharepaste — top-level convenience targets.
#
# The `desktop-*` targets are macOS only; Windows and Linux contributors should
# use `npm --prefix clients/desktop run build` instead.
#
# The `ios-*` targets run wherever `xtool` does, which in practice is WSL or
# Linux. They do not build the Rust half: `libsharepaste_ffi.a` is produced on
# `macos-latest` by CI, because cross-compiling vendored C SQLite for an Apple
# target needs Apple's own clang and sysroot. `ios-vendor` fetches it.

DESKTOP_DIR := clients/desktop
UI_DIR      := $(DESKTOP_DIR)/ui
TAURI_DIR   := $(DESKTOP_DIR)/src-tauri
BUNDLE_DIR  := $(TAURI_DIR)/target/release/bundle
APP_NAME    := sharepaste.app
APP_SRC     := $(BUNDLE_DIR)/macos/$(APP_NAME)
APP_DEST    := /Applications/$(APP_NAME)
DMG_DIR     := $(BUNDLE_DIR)/dmg

IOS_DIR     := clients/mobile/ios
# Which slice `ios-vendor` installs. The device one, because that is what
# `xtool dev` puts on the iPad; CI's own test job drops the simulator slice in
# the same place. Nothing in `Package.swift` knows which it got, and that is the
# whole mechanism that removes the need for an xcframework.
IOS_SLICE   ?= device
# The CI artifact holding both slices and the generated Swift bindings, from the
# job in `.github/workflows/desktop-build.yml`.
IOS_ARTIFACT := ios-static-library

.PHONY: help desktop-deps desktop-build desktop-install desktop-reinstall desktop-uninstall desktop-open desktop-dmg desktop-clean check-macos ios-vendor ios-build ios-run ios-clean

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
	@echo "  ios-vendor          Fetch the iOS static library + bindings from CI"
	@echo "                      (IOS_SLICE=device|simulator, default device)"
	@echo "  ios-build           xtool dev build (needs ios-vendor first)"
	@echo "  ios-run             xtool dev run — build, sign, install over USB"
	@echo "  ios-clean           Remove the fetched artifact and the build tree"

# -----------------------------------------------------------------------------
# iOS.
#
# A fresh clone cannot build the iOS app until this has run, and that is
# intended: the archive and the bindings are build *inputs* produced elsewhere,
# exactly as `libsharepaste_ffi.so` is on Android. The bindings come from the
# built library rather than from the sources, which is what makes a
# bindings/library mismatch impossible — so they travel together, from the same
# CI run, and are fetched together here.
# -----------------------------------------------------------------------------
ios-vendor:
	@command -v gh >/dev/null || { echo "gh is not installed; it is how the CI artifact is fetched"; exit 1; }
	@case "$(IOS_SLICE)" in device|simulator) ;; *) echo "IOS_SLICE must be device or simulator, got '$(IOS_SLICE)'"; exit 1 ;; esac
	@rm -rf $(IOS_DIR)/.vendor-download
	@mkdir -p $(IOS_DIR)/.vendor-download
	gh run download --name $(IOS_ARTIFACT) --dir $(IOS_DIR)/.vendor-download
	@mkdir -p $(IOS_DIR)/Vendor $(IOS_DIR)/Sources/sharepaste_ffiFFI $(IOS_DIR)/Sources/SharepasteCore
	@cp $(IOS_DIR)/.vendor-download/$(IOS_SLICE)/libsharepaste_ffi.a $(IOS_DIR)/Vendor/
	@cp $(IOS_DIR)/.vendor-download/generated/sharepaste_ffi.swift $(IOS_DIR)/Sources/SharepasteCore/
	@cp $(IOS_DIR)/.vendor-download/generated/sharepaste_ffiFFI.h $(IOS_DIR)/Sources/sharepaste_ffiFFI/
	@cp $(IOS_DIR)/.vendor-download/generated/sharepaste_ffiFFI.modulemap $(IOS_DIR)/Sources/sharepaste_ffiFFI/module.modulemap
	@rm -rf $(IOS_DIR)/.vendor-download
	@echo ">> vendored the $(IOS_SLICE) slice into $(IOS_DIR)/Vendor"

ios-build:
	@test -f $(IOS_DIR)/Vendor/libsharepaste_ffi.a || { echo "no archive in $(IOS_DIR)/Vendor — run 'make ios-vendor' first"; exit 1; }
	cd $(IOS_DIR) && xtool dev build

ios-run:
	@test -f $(IOS_DIR)/Vendor/libsharepaste_ffi.a || { echo "no archive in $(IOS_DIR)/Vendor — run 'make ios-vendor' first"; exit 1; }
	cd $(IOS_DIR) && xtool dev run

ios-clean:
	rm -rf $(IOS_DIR)/Vendor $(IOS_DIR)/Sources/sharepaste_ffiFFI $(IOS_DIR)/Sources/SharepasteCore
	rm -rf $(IOS_DIR)/.build $(IOS_DIR)/xtool

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
