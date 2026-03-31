.PHONY: help build test check clippy fmt clean sync feature release install

# Auto-generate version from today's date with auto-incrementing patch
# Format: YYYYMMDD.0.X where X increments if releasing multiple times per day
define get_next_version
$(shell \
	TODAY=$$(date +%Y%m%d); \
	LATEST=$$(git tag -l "v$$TODAY.*" 2>/dev/null | sort -V | tail -1); \
	if [ -z "$$LATEST" ]; then \
		echo "$$TODAY.0.0"; \
	else \
		PATCH=$$(echo "$$LATEST" | sed 's/.*\.0\.\([0-9]*\)/\1/'); \
		echo "$$TODAY.0.$$((PATCH + 1))"; \
	fi \
)
endef

VERSION := $(get_next_version)
BRANCH := $(shell git rev-parse --abbrev-ref HEAD)

help:
	@echo "Lok Makefile"
	@echo ""
	@echo "Development:"
	@echo "  make build                         - Build release binary"
	@echo "  make install                       - Build and install to cargo bin"
	@echo "  make test                          - Run tests"
	@echo "  make check                         - Run fmt check + clippy + test"
	@echo "  make clippy                        - Run clippy"
	@echo "  make fmt                           - Format code"
	@echo "  make clean                         - Clean build artifacts"
	@echo ""
	@echo "Workflow:"
	@echo "  make sync                          - Pull latest from upstream into main"
	@echo "  make feature NAME=my-feature       - Create feature branch from main"
	@echo "  make merge                         - Merge current feature branch into main"
	@echo ""
	@echo "Release:"
	@echo "  make release                       - Auto-version release ($(VERSION))"
	@echo "  make release VERSION=20260329.0.0  - Release with specific version"
	@echo ""
	@echo "Current branch: $(BRANCH)"
	@echo "Next version:   $(VERSION)"

# --- Development ---

build:
	cargo build --release

install: check
	cargo install --path .

test:
	cargo test

clippy:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

check: fmt
	cargo clippy -- -D warnings
	cargo test

clean:
	cargo clean

# --- Workflow ---

sync:
	@git checkout main
	@git fetch upstream
	@git merge upstream/main
	@git push origin main
	@echo "main synced with upstream and pushed to origin"

feature:
ifndef NAME
	$(error Usage: make feature NAME=my-feature)
endif
	@git checkout main
	@git checkout -b feature/$(NAME)
	@echo "Created feature/$(NAME) from main"

merge:
	@if [ "$(BRANCH)" = "main" ]; then echo "Already on main - switch to a feature branch first"; exit 1; fi
	@echo "Merging $(BRANCH) into main..."
	@git checkout main
	@git merge --no-ff $(BRANCH) -m "Merge $(BRANCH)"
	@echo "Merged. Run 'git push origin main' when ready."

# --- Release ---

release:
	@echo "Running checks before release..."
	@cargo fmt -- --check
	@cargo clippy -- -D warnings
	@cargo test
	@echo ""
	@echo "Creating release v$(VERSION)..."
	@git checkout -b release/v$(VERSION)
	@sed -i '' 's/^version = .*/version = "$(VERSION)"/' Cargo.toml
	@cargo check --quiet 2>/dev/null || true
	@git add Cargo.toml Cargo.lock
	@git commit -m "chore: bump version to $(VERSION)"
	@git checkout main
	@git merge --no-ff release/v$(VERSION) -m "Merge branch 'release/v$(VERSION)'"
	@git tag -a v$(VERSION) -m "Release v$(VERSION)"
	@git branch -d release/v$(VERSION)
	@cargo build --release
	@cp target/release/lok /usr/local/bin/lok
	@git push origin main
	@git push origin v$(VERSION)
	@echo ""
	@echo "Released v$(VERSION)"
	@echo "  - Tagged v$(VERSION)"
	@echo "  - Pushed to origin"
	@echo "  - Installed to /usr/local/bin/lok"
	@lok --version
