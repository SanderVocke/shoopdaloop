build/jack2/jackd \
build/jack2/client/libjack.so \
build/jack2/client/libjack.so.0 \
build/jack2/client/libjack.so.0.1 \
build/jack2/libjackserver.so \
build/jack2/libjackserver.so.0 \
build/jack2/libjackserver.so.1 \
build/jack2/jack_proxy.so \
&:
	cd third_party/jack2 && ./waf configure && ./waf
	mkdir -p build/jack2
	mkdir -p build/jack2/client
	cp third_party/jack2/build/jackd build/jack2/jackd
	cp third_party/jack2/build/common/libjack.so* build/jack2/client/
	cp third_party/jack2/build/common/libjackserver.so* build/jack2/
	cp third_party/jack2/build/jack_proxy.so build/jack2/

clean:
	rm -r third_party/jack2/build || true
	rm -r build || true