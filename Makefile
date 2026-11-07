# ==============================================================================
# kinun-signal AI Development Harness Makefile
# ==============================================================================
# Usage:
#   make check          Run all static & build harnesses (no-todo, backend, frontend)
#   make check-no-todo  Ensure no omitted code (TODO / FIXME / unimplemented! / todo!)
#   make check-backend  Run cargo fmt, cargo clippy (-D warnings), and cargo test
#   make check-frontend Run Next.js typecheck and production build
#   make smoke          Run end-to-end API smoke test against running backend
# ==============================================================================

.PHONY: check check-no-todo check-backend check-frontend smoke

check: check-no-todo check-backend check-frontend
	@echo "✅ All AI harness checks (no-todo, backend clippy+test, frontend build) passed!"

check-no-todo:
	@echo "🔍 [1/3] Checking for forbidden placeholders (TODO, FIXME, unimplemented!, todo!)..."
	@if grep -rnE "TODO|FIXME|unimplemented!\(\)|todo!\(\)" backend/src frontend/src; then \
		echo "❌ Forbidden placeholder found! AGENTS.md strictly prohibits omitted implementations."; \
		exit 1; \
	fi
	@echo "✅ No forbidden placeholders found."

check-backend:
	@echo "🦀 [2/3] Running Rust formatting, Clippy lints, and unit tests..."
	cd backend && cargo fmt --all -- --check
	cd backend && cargo clippy --all-targets -- -D warnings
	cd backend && cargo test

check-frontend:
	@echo "⚛️  [3/3] Running Next.js TypeScript check and production build..."
	cd frontend && npm run build

smoke:
	@bash scripts/smoke_test.sh
