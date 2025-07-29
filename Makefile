build:
	@cargo build

check:
	@cargo clippy --all-targets --all-features --tests --benches -- -D warnings

test:
	@cargo nextest run --all-features

coverage:
	@cargo tarpaulin --out Html --output-dir coverage
	@echo "Coverage report generated at coverage/tarpaulin-report.html"

coverage-open: coverage
	@open coverage/tarpaulin-report.html || xdg-open coverage/tarpaulin-report.html || echo "Please open coverage/tarpaulin-report.html in your browser"

release:
	@cargo release tag --execute
	@git cliff -o CHANGELOG.md
	@git commit -a -n -m "Update CHANGELOG.md" || true
	@git push origin master
	@cargo release push --execute

update-submodule:
	@git submodule update --init --recursive --remote

.PHONY: build test release update-submodule check coverage coverage-open
