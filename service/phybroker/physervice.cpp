#include "physervice.h"
#include "stddef.h"

#include <errno.h>
#include <fcntl.h>
#include <iostream>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <string>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <signal.h>
#include <string.h>
#include <pthread.h>
#include <errno.h>

//#include <rtdm/ipc.h>

//rt
#include <sys/mman.h>

// One of these must be defined, usually via the Makefile
#if defined(__APPLE__) || defined(__MACH__)
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

#if defined(LINUX)
static int pm_qos_fd = -1;

void start_low_latency(void)
{
    uint32_t target = 0;
    
    
    
    if (pm_qos_fd >= 0)
        return;
    pm_qos_fd = open("/dev/cpu_dma_latency", O_RDWR);
    if (pm_qos_fd < 0) {
        fprintf(stderr, "Failed to open PM QOS file: %s",
                strerror(errno));
        exit(errno);
    }
    write(pm_qos_fd, &target, sizeof(target));
}

void stop_low_latency(void)
{
    if (pm_qos_fd >= 0)
        close(pm_qos_fd);
}
#endif

// function prototypes
int open_port_and_set_baud_or_die(const char *name, long baud);
int write_bytes(int port, const char *data, int len);

int read_bytes(int port, char *data, int len);
void close_port(int port);
void delay(double sec);
void die(const char *format, ...) __attribute__ ((format (printf, 1, 2)));

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
    /*
     if (tcgetattr(fd, &tinfo) < 0) die("unable to get serial parms\n");
     cfmakeraw(&tinfo);
     if (cfsetspeed(&tinfo, baud) < 0) die("error in cfsetspeed\n");
     tinfo.c_cflag |= CLOCAL;
     if (tcsetattr(fd, TCSANOW, &tinfo) < 0) die("unable to set baud rate\n");
     fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) & ~O_NONBLOCK);
     */
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

inline int read_bytes(int port, char *data, int len)
{
    return read(port, data, len);
}

inline int write_bytes(int port, const char *data, int len)
{
#if defined(MACOSX) || defined(LINUX)
    int r = write(port, data, len);
    return r ;
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

int port;
double sum=0.0;

bool run = true ;

static void *transfer(void *arg) {
    struct timeval begin, end;
    double elapsed = 0.0 ;
    double elapsed_iter = 0.0 ;
 
    PhybrokerSrv lPhybrokerSrv = PhybrokerSrv() ;

    char * out_buffer = lPhybrokerSrv.getOutDataHandler() ;
    char * in_buffer = lPhybrokerSrv.getInDataHandler() ;
    
    while (1) {
        gettimeofday(&begin, NULL);
        int n_in = write_bytes(port, out_buffer, out_data_size);
        //usleep(400);
        int n_out = read_bytes(port, in_buffer, in_data_size);
        gettimeofday(&end, NULL);
        
        elapsed = (double)(end.tv_sec - begin.tv_sec);
        elapsed += (double)(end.tv_usec - begin.tv_usec) / 1000000.0;
        sum += elapsed ;
        elapsed_iter += elapsed ;
        begin.tv_sec = end.tv_sec;
        begin.tv_usec = end.tv_usec;

        //int cmp = memcmp(in_buffer + 20, out_buffer + 20, size - 20) ;
        //		std::cout << getHexString((unsigned char*)&(in_buffer[4]),16) << std::endl ;
        /*
         if ( cmp != 0 )
         {
         printf("compare error %d\n", cmp) ;
         std::cout << getHexString((unsigned char *)out_buffer,size) << std::endl ;
         std::cout << getHexString((unsigned char*)in_buffer,size) << std::endl ;
         }
         */

        /*
        if((count % 4096) == 0) {
            printf("Packets per second = Packets(%u) Elapsed(%.12g:%.12g) PacketI/Elapsed(%.12g) Packet/Elapsed(%.12g)\n", packet_n, elapsed, sum, 4096/elapsed_iter, packet_n/sum);
            elapsed_iter = 0.0;
        }
        */
    }
}

int main(int argc, char **argv)
{
    pthread_t svtid ;
#if defined(LINUX)
    start_low_latency();
#endif
    sigset_t set;
    int sig;
    
    sigemptyset(&set);
    sigaddset(&set, SIGINT);
    sigaddset(&set, SIGTERM);
    sigaddset(&set, SIGHUP);
    pthread_sigmask(SIG_BLOCK, &set, NULL);
    
    mlockall(MCL_CURRENT|MCL_FUTURE);
    
    if (argc < 2) die("Usage: receive_test <comport>\n");
    if (argc == 2) {
        port = open_port_and_set_baud_or_die(argv[1], BAUD);
        printf("port %s opened\n", argv[1]);
    } else {
        printf("Exiting: wrong argument number %d\n", argc);
        exit(10) ;
    }

    /*
     int iRet;
     pthread_mutexattr_t csAttr;
     iRet = pthread_mutexattr_init(&csAttr);
     if (iRet == 0)
     iRet = pthread_mutexattr_settype(&csAttr, PTHREAD_MUTEX_ERRORCHECK);
     if (iRet == 0)
     iRet = pthread_mutexattr_setprotocol(&csAttr,
     PTHREAD_PRIO_INHERIT); // error: PTHREAD_PRIO_INHERIT undeclared
     */
    
    pthread_attr_t svattr;
    pthread_attr_init(&svattr);
    pthread_attr_setschedpolicy(&svattr, SCHED_FIFO);
    pthread_attr_setdetachstate(&svattr, PTHREAD_CREATE_JOINABLE);
    pthread_attr_setinheritsched(&svattr, PTHREAD_EXPLICIT_SCHED);
    
    struct sched_param svparam = {.sched_priority = 99 };
    pthread_attr_setschedparam(&svattr, &svparam);
    
    errno = pthread_create(&svtid, &svattr, &transfer, NULL);
    if (errno)
        printf("pthread_create\n");
    
    //	sigwait(&set, &sig);
    //	pthread_cancel(svtid);
    pthread_join(svtid, NULL);
    
    //pthread_mutexattr_destroy(&csAttr);
    close_port(port);
    return 0;
}

