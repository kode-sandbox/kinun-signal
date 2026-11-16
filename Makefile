# ==============================================================================
# kinun-signal AI Development Harness Makefile
# ==============================================================================
# Usage:
#   make agent-context-map  Print line-numbered signature map of large files
#   make check              Run all static & build harnesses (no-todo, backend, US/JP frontends)
#   make check-no-todo      Ensure no omitted code (TODO / FIXME / unimplemented! / todo!)
#   make check-backend      Run cargo fmt, cargo clippy (-D warnings), cargo test, and cargo build
#   make check-frontend     Run US Next.js typecheck and production build
#   make check-frontend-jp  Run JP Next.js typecheck and production build
#   make smoke              Run end-to-end API smoke test against running backend
# ==============================================================================

.PHONY: agent-context-map check check-no-todo check-backend check-frontend check-frontend-jp smoke

agent-context-map:
	@echo "🗺️  === AI Agent Context Map (Use StartLine/EndLine to read sections) ==="
	@echo "\n--- [AGENTS.md Headings] ---"
	@grep -nE "^#{1,3} " AGENTS.md
	@echo "\n--- [backend/src/models.rs Signatures] ---"
	@grep -nE "^pub (struct|enum|fn|type) " backend/src/models.rs
	@echo "\n--- [backend/src/services/fmp.rs Signatures] ---"
	@grep -nE "pub (async )?fn |pub (struct|enum) " backend/src/services/fmp.rs
	@echo "\n--- [backend/src/services/llm.rs Signatures] ---"
	@grep -nE "pub (async )?fn |pub (struct|enum) " backend/src/services/llm.rs
	@if [ -f backend/src/services/edinet.rs ] || [ -f backend/src/services/jquants.rs ]; then \
		echo "\n--- [backend/src/services/edinet.rs & jquants.rs Signatures] ---"; \
		grep -nE "pub (async )?fn |pub (struct|enum) " $$(ls backend/src/services/edinet.rs backend/src/services/jquants.rs 2>/dev/null); \
	fi
	@echo "\n--- [backend/src/handlers.rs Signatures] ---"
	@grep -nE "^pub async fn " backend/src/handlers.rs
	@if [ -f backend/src/jp_handlers.rs ]; then \
		echo "\n--- [backend/src/jp_handlers.rs Signatures] ---"; \
		grep -nE "^pub async fn " backend/src/jp_handlers.rs; \
	fi

check: check-no-todo check-backend check-frontend check-frontend-jp
	@echo "✅ All AI harness checks (no-todo, backend clippy+test, frontend builds) passed!"

check-no-todo:
	@echo "🔍 [1/4] Checking for forbidden placeholders (TODO, FIXME, unimplemented!, todo!)..."
	@TARGETS="backend/src frontend/src"; \
	if [ -d frontend-jp/src ]; then TARGETS="$$TARGETS frontend-jp/src"; fi; \
	if grep -rnE "TODO|FIXME|unimplemented!\(\)|todo!\(\)" $$TARGETS; then \
		echo "❌ Forbidden placeholder found! AGENTS.md strictly prohibits omitted implementations."; \
		exit 1; \
	fi
	@echo "✅ No forbidden placeholders found in checked directories."

check-backend:
	@echo "🦀 [2/4] Running Rust formatting, Clippy lints, unit tests, and build..."
	cd backend && cargo fmt --all -- --check
	cd backend && cargo clippy --all-targets -- -D warnings
	cd backend && cargo test
	cd backend && cargo build

check-frontend:
	@echo "⚛️  [3/4] Running US Next.js TypeScript check and production build..."
	cd frontend && npx tsc --noEmit && npm run build

check-frontend-jp:
	@if [ -f frontend-jp/package.json ]; then \
		echo "🇯🇵 [4/4] Running JP Next.js TypeScript check and production build..."; \
		cd frontend-jp && npx tsc --noEmit && npm run build; \
	else \
		echo "🇯🇵 [4/4] frontend-jp not present on this branch; skipping."; \
	fi

smoke:
	@bash scripts/smoke_test.sh
