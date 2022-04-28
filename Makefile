.PHONY: all clean

all:
	cargo build
	cp target/debug/3700send ./
	cp target/debug/3700recv ./

clean:
	cargo clean
	rm 3700send
	rm 3700recv