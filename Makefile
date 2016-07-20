# You can always use Cargo directly. But this makefile helps by:
# 1) doing the operation on both the main project and the 'app_tests' sub-project
# 2) including examples in build and check operations
# 3) sym-linking binaries into bin/


EXAMPLES = hello pippincmd
APP_EXAMPLES = sequences
C_B = cargo build
C_B_EX = $(C_B) --example $$ex
C_C = cargo check
C_C_EX = $(C_C) --example $$ex
C_T = cargo test

.PHONY:	build check test clean links

build:	links
	@echo "———  main project  ———" && \
	echo "→ $(C_B)" ; $(C_B) && \
	for ex in $(EXAMPLES); do \
		echo "→ $(C_B_EX)" && \
		$(C_B_EX) ; \
	done && \
	echo "———  app_tests  ———" && \
	cd app_tests && \
	echo "→ $(C_B)" && $(C_B) && \
	for ex in $(APP_EXAMPLES); do \
		echo "→ $(C_B_EX)" && \
		$(C_B_EX) ; \
	done

check:
	@echo "———  main project  ———" && \
	echo "→ $(C_C)" && $(C_C) && \
	for ex in $(EXAMPLES); do \
		echo "→ $(C_C_EX)" && \
		$(C_C_EX) ; \
	done && \
	echo "———  app_tests  ———" && \
	cd app_tests && \
	echo "→ $(C_C)" && $(C_C) && \
	for ex in $(APP_EXAMPLES); do \
		echo "→ $(C_C_EX)" && \
		$(C_C_EX) ; \
	done

test:	links
	@echo "———  main project  ———" && \
	echo "→ $(C_T)" && $(C_T) && \
	echo "———  app_tests  ———" && \
	cd app_tests && \
	echo "→ $(C_T)" && $(C_T)

clean:
	cargo clean && \
	cd app_tests && cargo clean

links:
	@mkdir -p bin
	@for ex in $(EXAMPLES); do \
		test -L bin/$$ex && (test -e bin/$$ex || rm bin/$$ex) ; \
		test -L bin/$$ex || ( \
			echo "Creating symlink bin/$$ex" && \
			ln -s ../target/debug/examples/$$ex bin/$$ex ) ; \
	done
	@for ex in $(APP_EXAMPLES); do \
		test -L bin/$$ex && (test -e bin/$$ex || rm bin/$$ex) ; \
		test -L bin/$$ex || ( \
			echo "Creating symlink bin/$$ex" && \
			ln -s ../app_tests/target/debug/examples/$$ex bin/$$ex ) ; \
	done
