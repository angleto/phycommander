#include <stdio.h>
#include <signal.h>
#include <unistd.h>
#include <sys/mman.h>

#include <trank/native/task.h>
#include <trank/native/timer.h>

#if defined(LINUX)
#include <sys/ioctl.h>
#include <linux/serial.h>
#include <termios.h>
#include <sys/select.h>
#define BAUD B460800
#endif

RT_TASK demo_task;

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

/* NOTE: error handling omitted. */

void demo(void *arg)
{
        RTIME now, previous;

        /*
         * Arguments: &task (NULL=self),
         *            start time,
         *            period (here: 1 s)
         */
        rt_task_set_periodic(NULL, TM_NOW, 100000);
        previous = rt_timer_read();

        while (1) {
                rt_task_wait_period(NULL);
                now = rt_timer_read();

                /*
                 * NOTE: printf may have unexpected impact on the timing of
                 *       your program. It is used here in the critical loop
                 *       only for demonstration purposes.
                 */
                printf("Time since last turn: %ld.%06ld ms\n",
                       (long)(now - previous) / 1000000,
                       (long)(now - previous) % 1000000);
                       previous = now;
        }
}

void catch_signal(int sig)
{
}

int main(int argc, char* argv[])
{
        signal(SIGTERM, catch_signal);
        signal(SIGINT, catch_signal);

        /* Avoids memory swapping for this program */
        mlockall(MCL_CURRENT|MCL_FUTURE);

        /*
         * Arguments: &task,
         *            name,
         *            stack size (0=default),
         *            priority,
         *            mode (FPU, start suspended, ...)
         */
        rt_task_create(&demo_task, "trivial", 0, 99, 0);

        /*
         * Arguments: &task,
         *            task function,
         *            function argument
         */
        rt_task_start(&demo_task, &demo, NULL);

        pause();

        rt_task_delete(&demo_task);
}

