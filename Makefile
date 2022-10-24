third_party/sooperlooper/libs/midi++/libmidipp.a: third_party/sooperlooper/Makefile
	cd third_party/sooperlooper/libs/midi++ && make libmidipp.a

third_party/sooperlooper/libs/pbd/libpbd.a: third_party/sooperlooper/Makefile
	cd third_party/sooperlooper/libs/pbd && make libpbd.a

third_party/sooperlooper/Makefile: third_party/sooperlooper/configure
	cd third_party/sooperlooper && ./configure

third_party/sooperlooper/configure:
	cd third_party/sooperlooper && ./autogen.sh

third_party/sooperlooper/src/sooperlooper: third_party/sooperlooper/Makefile third_party/sooperlooper/libs/pbd/libpbd.a third_party/sooperlooper/libs/midi++/libmidipp.a
	cd third_party/sooperlooper/src && make sooperlooper

build/sooperlooper: third_party/sooperlooper/src/sooperlooper
	mkdir -p build
	cp third_party/sooperlooper/src/sooperlooper build/

clean:
	rm -r build || true
	cd third_party/sooperlooper && make clean || true
