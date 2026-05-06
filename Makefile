PREFIX ?= /usr/local
BINDIR ?= $(PREFIX)/bin
BIN ?= cpuwatch
INSTALL_PATH ?= $(BINDIR)/$(BIN)
CAPABILITY ?= cap_dac_read_search+ep

CARGO ?= cargo
INSTALL ?= install
SETCAP ?= setcap
GETCAP ?= getcap

ifeq ($(shell id -u),0)
SUDO ?=
else
SUDO ?= sudo
endif

BUILD_BIN := target/release/$(BIN)

.PHONY: all build install ensure-install-build install-binary capability show-capability uninstall test fmt clippy check clean

all: build

build:
	$(CARGO) build --release

install: install-binary
	$(SUDO) $(SETCAP) $(CAPABILITY) $(INSTALL_PATH)
	$(GETCAP) $(INSTALL_PATH)

ensure-install-build:
	@if [ "$$(id -u)" -eq 0 ]; then \
		test -x "$(BUILD_BIN)" || { \
			echo "$(BUILD_BIN) is missing; run 'make build' before 'sudo make install'."; \
			exit 1; \
		}; \
	else \
		$(MAKE) build; \
	fi

install-binary: ensure-install-build
	$(SUDO) $(INSTALL) -d $(dir $(INSTALL_PATH))
	$(SUDO) $(INSTALL) -m 0755 $(BUILD_BIN) $(INSTALL_PATH)

capability:
	$(SUDO) $(SETCAP) $(CAPABILITY) $(INSTALL_PATH)
	$(GETCAP) $(INSTALL_PATH)

show-capability:
	$(GETCAP) $(INSTALL_PATH)

uninstall:
	$(SUDO) rm -f $(INSTALL_PATH)

test:
	$(CARGO) test

fmt:
	$(CARGO) fmt --check

clippy:
	$(CARGO) clippy -- -D warnings

check: fmt test clippy

clean:
	$(CARGO) clean
