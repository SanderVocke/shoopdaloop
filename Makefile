build/sooperlooper: FORCE
	cd third_party/sooperlooper && ./autogen.sh && ./configure
	cd third_party/sooperlooper/src && make sooperlooper
	mkdir -p build
	cp third_party/sooperlooper/src/sooperlooper build/

FORCE:

clean:
	rm -r build || true
	cd third_party/sooperlooper && make clean || true
