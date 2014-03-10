#include <errno.h>
#include <fcntl.h>
#include <iostream>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>

// One of these must be defined, usually via the Makefile
#if defined(__APPLE__) && defined(__MACH__)
#define MACOSX
#else
#define LINUX
#endif

#if defined(MACOSX)
#include <termios.h>
#include <sys/select.h>
#define BAUD 460800
#endif

#if defined(LINUX)
#include <sys/ioctl.h>
#include <linux/serial.h>
#include <termios.h>
#include <sys/select.h>
#define BAUD B460800
#endif

// function prototypes
int open_port_and_set_baud_or_die(const char *name, long baud);
int write_bytes(int port, const char *data, int len);
int read_bytes(int port, char *data, int len);
void close_port(int port);
void delay(double sec);
void die(const char *format, ...) __attribute__ ((format (printf, 1, 2)));

char out_buffer[100000];
char in_buffer[100000];

std::string getHexString(unsigned char const * const pBuffer, const size_t pLength)
{
	static const char* const lHexLut = "0123456789ABCDEF";
	std::string lString ;
	for ( size_t i = 0 ; i < pLength ; i ++ ) {
		const unsigned char lChar = pBuffer[i] ;
		lString.push_back(lHexLut[lChar >> 4]);
		lString.push_back(lHexLut[lChar & 15]);
	}
	return lString ;
}

int main(int argc, char **argv)
{
	int port;
	struct timeval begin, end;
	int size = 30000;
	int i, n_in, n_out=0;
	double elapsed, sum=0.0;

	if (argc < 2) die("Usage: receive_test <comport>\n       receive_test <blocksize> <comport>\n");
	if (argc == 2) {
		port = open_port_and_set_baud_or_die(argv[1], BAUD);
		printf("port %s opened\n", argv[1]);
	} else {
		if (sscanf(argv[1], "%d", &size) != 1 ||
				size < 1 || size > sizeof(out_buffer)) {
			die("Usage: receive_test <blocksize> <comport>\n");
		}
		port = open_port_and_set_baud_or_die(argv[2], BAUD);
		std::cout << "port opened: " << argv[2] << std::endl ;
	}

	size_t packet_n = 1024 * 1024 ;
	for (size_t count = 0; count < packet_n ; count++) {
		for (i=0; i < size; i++) {
			out_buffer[i] = rand();
		}
		if(count % 2 == 0)
		{
		out_buffer[2] = 0x0F ;
		out_buffer[3] = 0xF0 ;
		} else {
		out_buffer[2] = 0xF0 ;
		out_buffer[3] = 0x0F ;
		}
		gettimeofday(&begin, NULL);
		n_in = write_bytes(port, out_buffer, size);
		n_out = read_bytes(port, in_buffer, size);
		if (n_out != size && n_in != size )
			die("errors transmitting data\n");
		gettimeofday(&end, NULL);

		elapsed = (double)(end.tv_sec - begin.tv_sec);
		elapsed += (double)(end.tv_usec - begin.tv_usec) / 1000000.0;
		sum += elapsed ;
		begin.tv_sec = end.tv_sec;
		begin.tv_usec = end.tv_usec;

		int cmp = memcmp(in_buffer + 2, out_buffer + 2, size - 2) ; 
		if ( cmp > 0 )
		{
			printf("compare error %d\n", cmp) ;
			std::cout << getHexString((unsigned char *)out_buffer,size) << std::endl ;
			std::cout << getHexString((unsigned char*)in_buffer,size) << std::endl ;
		}
	}
	close_port(port);
	printf("Packets per second = Packets(%u) Elapsed(%.12g) Packet/Elapsed(%.12g)\n", packet_n, sum, packet_n/sum);
	return 0;
}


/**********************************/
/*  Serial Port Functions         */
/**********************************/


int open_port_and_set_baud_or_die(const char *name, long baud)
{
	int fd;
#if defined(MACOSX)
	struct termios tinfo;
	fd = open(name, O_RDWR | O_NONBLOCK);
	if (fd < 0) die("unable to open port %s\n", name);
	if (tcgetattr(fd, &tinfo) < 0) die("unable to get serial parms\n");
	cfmakeraw(&tinfo);
	if (cfsetspeed(&tinfo, baud) < 0) die("error in cfsetspeed\n");
	tinfo.c_cflag |= CLOCAL;
	if (tcsetattr(fd, TCSANOW, &tinfo) < 0) die("unable to set baud rate\n");
	fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) & ~O_NONBLOCK);
#elif defined(LINUX)
	struct termios tinfo;
	struct serial_struct kernel_serial_settings;
	int r;
	fd = open(name, O_RDWR);
	if (fd < 0) die("unable to open port %s\n", name);
	if (tcgetattr(fd, &tinfo) < 0) die("unable to get serial parms\n");
	cfmakeraw(&tinfo);
	if (cfsetspeed(&tinfo, baud) < 0) die("error in cfsetspeed\n");
	if (tcsetattr(fd, TCSANOW, &tinfo) < 0) die("unable to set baud rate\n");
	r = ioctl(fd, TIOCGSERIAL, &kernel_serial_settings);
	if (r >= 0) {
		kernel_serial_settings.flags |= ASYNC_LOW_LATENCY;
		r = ioctl(fd, TIOCSSERIAL, &kernel_serial_settings);
		if (r >= 0) printf("set linux low latency mode\n");
	}
#endif
	return fd;

}

int read_bytes(int port, char *data, int len)
{
	return read(port, data, len);
}

int write_bytes(int port, const char *data, int len)
{
#if defined(MACOSX) || defined(LINUX)
	return write(port, data, len);
#elif defined(WINDOWS)
	DWORD n;
	BOOL r;
	r = WriteFile(port, data, len, &n, NULL);
	if (!r) return 0;
	return n;
#endif
}


void close_port(int port)
{
#if defined(MACOSX) || defined(LINUX)
	close(port);
#elif defined(WINDOWS)
	CloseHandle(port);
#endif
}


/**********************************/
/*  Misc. Functions               */
/**********************************/

void delay(double sec)
{
#if defined(MACOSX) || defined(LINUX)
	usleep(sec * 1000000);
#elif defined(WINDOWS)
	Sleep(sec * 1000);
#endif
}


void die(const char *format, ...)
{
	va_list args;
	va_start(args, format);
	vfprintf(stderr, format, args);
	exit(1);
}

