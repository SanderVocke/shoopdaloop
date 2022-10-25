SOURCES := $(shell find third_party/sooperlooper/src -name '*.cpp' -o -name '*.c' -o -name '*.cc')

third_party/sooperlooper/Makefile: third_party/sooperlooper/configure
	cd third_party/sooperlooper && ./configure

third_party/sooperlooper/configure:
	cd third_party/sooperlooper && ./autogen.sh

third_party/sooperlooper/src/sooperlooper: third_party/sooperlooper/Makefile $(SOURCES)
	cd third_party/sooperlooper/libs/midi++ && make libmidipp.a
	cd third_party/sooperlooper/libs/pbd && make libpbd.a
	cd third_party/sooperlooper/src && make sooperlooper

build/sooperlooper: third_party/sooperlooper/src/sooperlooper
	mkdir -p build
	cp third_party/sooperlooper/src/sooperlooper build/

.PHONY: clean
clean:
	rm -r build || true
	cd third_party/sooperlooper && make clean || true
