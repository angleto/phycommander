#include <unistd.h>
#include <stdlib.h>
#include <stdio.h>
#include <errno.h>
#include <signal.h>
#include <errno.h>
#include <libusb.h>
#include <iostream>
#include <string.h>

//Endpoint data
#define EP_ISO_OUT	0x02
#define EP_ISO_IN	0x81 | 0x80

using namespace std;

static void callbackIn(struct libusb_transfer *transfer)
{
        printf("Received.\n");
       // libusb_submit_transfer(transfer); // repeat
}

static void callbackOut(struct libusb_transfer *transfer)
{
        printf("Sent.\n");
       // libusb_submit_transfer(transfer); // repeat
}

int main(void)
{
    libusb_device **devs;
    libusb_device_handle *dev_handle;
    libusb_context *ctx = NULL;
    int ret;
    ssize_t cnt;

    ret = libusb_init(&ctx);

    if (ret < 0){
        cout << "Initialization Error " << ret << endl;
        return 1;
    }

    //libusb_set_debug(ctx, LIBUSB_LOG_LEVEL_DEBUG);
//    libusb_set_debug(ctx, 0);

    cnt = libusb_get_device_list(ctx,&devs);

    if (cnt < 0){
        cout << "Get Device Error" << endl;
        return 1;
    }

    cout << cnt << " Devices in list. " << endl;

    dev_handle = libusb_open_device_with_vid_pid(ctx, 0x03eb, 0x2423);

    if (dev_handle == NULL)
        cout << "Cannot open device" << endl;
    else
        cout << "Device opened" << endl;

    libusb_free_device_list(devs,1);

    if (libusb_kernel_driver_active(dev_handle,0) == 1){
        cout << "Kernel Driver Active" << endl;
        if (libusb_detach_kernel_driver(dev_handle,0) == 0)
            cout << "Kernel Driver Detached !" << endl;
    }

    ret = libusb_set_configuration(dev_handle, 1); //bConfigurationValue


    if (ret < 0){
        cout << "Configuration Not Set" << endl;
        return 1;
    }

    cout << "Configuration Set" << endl;

    ret = libusb_claim_interface(dev_handle, 0); //bInterfaceNumber

    if (ret < 0){
        cout << "Cannot Claim Interface" << endl;
        return 1;
    }

    cout << "Interface claimed" << endl;

	// set isochronous setting
    ret = libusb_set_interface_alt_setting(dev_handle,
			0, //bInterfaceNumber
			1 // bAlternateSetting
			);

    if (ret < 0){
        cout << "Cannot Set Alternate Setting: " << libusb_error_name(ret) << std::endl ;
        return 1;
    }

    cout << "Alternate Setting Set" << endl;

    ret = 0;
	
	size_t count = 0 ;
	uint8_t *out_buffer = (uint8_t*) malloc(64);
	uint8_t *in_buffer = (uint8_t*) malloc(64);
	while (ret >= 0){
		if(count % 2 == 0)
		{
			out_buffer[2] = 0x0F ;
			out_buffer[3] = 0xF0 ;
		} else {
			out_buffer[2] = 0xF0 ;
			out_buffer[3] = 0x0F ;
		}

		*((uint16_t *)(&(out_buffer[4]))) = (uint16_t) (count % 4096) ;
		*((uint16_t *)(&(out_buffer[6]))) = (uint16_t) (4095 - (count % 4096)) ;
		*((uint32_t *)(&(out_buffer[8]))) = (uint32_t) (count) ;

		count += 1 ;

		struct libusb_transfer *xfer;
		xfer = libusb_alloc_transfer(1);

		if(xfer == NULL)
		{
			printf("Error: fail to allocate transfer 0\n");
		}

		libusb_fill_iso_transfer(
				xfer,
				dev_handle,
				EP_ISO_OUT, // Interface 3 Alt 1 Endpoint 0 Address 0x82
				out_buffer,
				64,
				1,
				callbackOut,
				NULL, 
				100 // TIMEOUT
		);

		if ( (ret = libusb_submit_transfer(xfer)) != 0)
		{
			fprintf(stderr, "failed to submit transfer 0 : %d : %s\n", ret, libusb_error_name(ret));
		} else {
			//printf("OK\n");
			//Here need to be some synchronous read function
		}

		struct libusb_transfer *xferIn;
		xferIn = libusb_alloc_transfer(1);

		if(xferIn == NULL)
		{
			printf("Error: fail to allocate transfer 1\n");
		}

		libusb_fill_iso_transfer(
				xferIn,
				dev_handle,
				EP_ISO_IN, // Interface 2 Alt 1 Endpoint 0 Address 0x82
				in_buffer,
				64,
				1,
				callbackIn,
				NULL, 
				100 // TIMEOUT
			);

		if ( (ret = libusb_submit_transfer(xferIn)) != 0)
		{
			fprintf(stderr, "failed to submit transfer 1 : %d : %s\n", ret, libusb_error_name(ret));
		} else {
			//printf("OK\n");
			//Here need to be some synchronous read function
		}
	}

	free(in_buffer) ;
	free(out_buffer) ;

    if (ret < 0)
        cout << "Error transferring the data" << endl;

    return 0;
}

