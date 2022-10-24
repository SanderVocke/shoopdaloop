configure:
	cd third_party/sooperlooper && ./autogen.sh && ./configure

build/sooperlooper:
	cd third_party/sooperlooper/libs/pbd && make libpbd.a
	cd third_party/sooperlooper/libs/midi++ && make libmidipp.a
	cd third_party/sooperlooper/src && make sooperlooper
	mkdir -p build
	cp third_party/sooperlooper/src/sooperlooper build/

clean:
	rm -r build || true