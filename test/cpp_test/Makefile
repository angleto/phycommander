CC = gcc
CXX = g++
CFLAGS = -Wall -O3
TARGETS = memory_lock iso_transf_test iso_test sam3u_benchmark testusb iso receive_test_rtai receive_thread_rtai

all: $(TARGETS)

receive_test_rtai: receive_test.c Makefile
	$(CXX) $(CFLAGS) -o receive_test_rtai receive_test.c -L/usr/realtime/lib -L/usr/xenomai/lib -I/usr/realtime/include -I/usr/realtime/include -I/usr/xenomai/include/cobalt -I/usr/xenomai/include -lpthread -lcobalt 

receive_thread_rtai: receive_thread.c Makefile
	$(CXX) $(CFLAGS) -o receive_thread_rtai receive_thread.c -L/usr/src/linux -L/usr/src/linux/include -L/usr/realtime/lib -I/usr/realtime/include -I/usr/xenomai/include -lrt -llxrt -lpthread

receive_test: receive_test.c Makefile
	$(CXX) $(CFLAGS) -o receive_test receive_test.c -lrt -lpthread

memory_lock: memory_lock.c Makefile
	$(CXX) $(CFLAGS) -o memory_lock memory_lock.c -lrt -lpthread

iso_test: iso_test.cpp Makefile
	 $(CXX) $(CFLAGS) iso_test.cpp -I/usr/include/libusb-1.0 -o iso_test -lusb-1.0

iso_transf_test: iso_transf_test.cpp Makefile
	 $(CXX) $(CFLAGS) iso_transf_test.cpp -I/usr/include/libusb-1.0 -o iso_transf_test -lusb-1.0

sam3u_benchmark: sam3u_benchmark.cpp Makefile
	 $(CXX) $(CFLAGS) sam3u_benchmark.cpp -I/usr/include/libusb-1.0 -o sam3u_benchmark -lusb-1.0

testusb: testusb.c Makefile
	 gcc $(CFLAGS) testusb.c -I/usr/include/libusb-1.0 -o testusb -lusb-1.0 -lpthread

iso: iso.c Makefile
	 $(CXX) $(CFLAGS) iso.c -I/usr/include/libusb-1.0 -o iso -lusb-1.0 -lpthread

clean:
	rm -f $(TARGETS)

