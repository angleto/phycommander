/**
 * \file
 *
 * \brief Empty user application template
 *
 */

/**
 * \mainpage User Application template doxygen documentation
 *
 * \par Empty user application template
 *
 * Bare minimum empty user application template
 *
 * \par Content
 *
 * -# Include the ASF header files (through asf.h)
 * -# Minimal main function that starts with a call to board_init()
 * -# "Insert application code here" comment
 *
 */

/*
 * Include header files for all drivers that have been imported from
 * Atmel Software Framework (ASF).
 */
#include <asf.h>
#include <string.h>

#define MSG_SIZE 64
static const iram_size_t sSize = MSG_SIZE ;
static uint8_t sIn[MSG_SIZE] ;
static uint8_t sOut[MSG_SIZE] ;

bool main_callback_cdc_enable(void)
{
//	gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_14) ;

	//my_flag_autorize_cdc_transfert = true;
	return true;
}

void main_callback_cdc_disable(void)
{
//	gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_14) ;

	//my_flag_autorize_cdc_transfert = false;
}

void my_callback_rx_notify(uint8_t port)
{
//	gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_15) ;
//	gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_15) ;
}

void my_callback_tx_empty_notify(uint8_t port)
{
//	gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_2) ;
//	gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_2) ;	
}


void my_callback_config(uint8_t port, usb_cdc_line_coding_t * cfg)
{
//	gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_3) ;
//	gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_3) ;		
}


void my_callback_cdc_set_dtr(uint8_t port, bool b_enable)
{
//	gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_4) ;
//	gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_4) ;
}

void my_callback_cdc_set_rts(uint8_t port, bool b_enable)
{
//	gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_5) ;
//	gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_5) ;
}

Pio * sDigInPorts[PHYCMD_DIGITAL_INPUT_NUM] ;
Pio * sDigOutPorts[PHYCMD_DIGITAL_OUTPUT_NUM] ;

void initDigInPorts(Pio * pPioPtrArray[])
{
	pPioPtrArray[0] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_0 >> 5)));
	pPioPtrArray[1] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_1 >> 5)));
	pPioPtrArray[2] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_2 >> 5)));
	pPioPtrArray[3] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_3 >> 5)));
	pPioPtrArray[4] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_4 >> 5)));
	pPioPtrArray[5] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_5 >> 5)));
	pPioPtrArray[6] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_6 >> 5)));
	pPioPtrArray[7] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_7 >> 5)));
	pPioPtrArray[8] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_8 >> 5)));
	pPioPtrArray[9] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_9 >> 5)));
	pPioPtrArray[10] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_10 >> 5)));
	pPioPtrArray[11] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_11 >> 5)));
	pPioPtrArray[12] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_12 >> 5)));
	pPioPtrArray[13] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_13 >> 5)));
	pPioPtrArray[14] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_14 >> 5)));
	pPioPtrArray[15] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_INPUT_15 >> 5)));	
}

void initDigOutPorts(Pio * pPioPtrArray[])
{
	pPioPtrArray[0] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_0 >> 5)));
	pPioPtrArray[1] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_1 >> 5)));
	pPioPtrArray[2] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_2 >> 5)));
	pPioPtrArray[3] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_3 >> 5)));
	pPioPtrArray[4] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_4 >> 5)));
	pPioPtrArray[5] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_5 >> 5)));
	pPioPtrArray[6] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_6 >> 5)));
	pPioPtrArray[7] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_7 >> 5)));
	pPioPtrArray[8] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_8 >> 5)));
	pPioPtrArray[9] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_9 >> 5)));
	pPioPtrArray[10] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_10 >> 5)));
	pPioPtrArray[11] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_11 >> 5)));
	pPioPtrArray[12] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_12 >> 5)));
	pPioPtrArray[13] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_13 >> 5)));
	pPioPtrArray[14] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_14 >> 5)));
	pPioPtrArray[15] = (Pio *)((uint32_t)PIOA + (PIO_DELTA * (PHYCMD_DIGITAL_OUTPUT_15 >> 5)));
}

__attribute__((always_inline))
inline
void getDigInValue(uint16_t * const pValue)
{
	uint16_t lIn = 0 ;
	uint16_t lInTmp = 0 ;
	lInTmp = (sDigInPorts[0]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_0 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<0) ;
	lInTmp = (sDigInPorts[1]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_1 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<1) ;
	lInTmp = (sDigInPorts[2]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_2 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<2) ;
	lInTmp = (sDigInPorts[3]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_3 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<3) ;
	lInTmp = (sDigInPorts[4]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_4 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<4) ;
	lInTmp = (sDigInPorts[5]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_5 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<5) ;
	lInTmp = (sDigInPorts[6]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_6 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<6) ;
	lInTmp = (sDigInPorts[7]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_7 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<7) ;
	lInTmp = (sDigInPorts[8]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_8 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<8) ;
	lInTmp = (sDigInPorts[9]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_9 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<9) ;
	lInTmp = (sDigInPorts[10]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_10 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<10) ;
	lInTmp = (sDigInPorts[11]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_11 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<11) ;
	lInTmp = (sDigInPorts[12]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_12 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<12) ;
	lInTmp = (sDigInPorts[13]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_13 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<13) ;
	lInTmp = (sDigInPorts[14]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_14 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<14) ;
	lInTmp = (sDigInPorts[15]->PIO_PDSR >> (PHYCMD_DIGITAL_INPUT_15 & 0x1F)) & 1;
	lIn = lIn | (lInTmp <<15) ;
	*pValue = lIn ;
}

__attribute__((always_inline))
inline
void setDigOutValue(const uint16_t pValue)
{
	if((( pValue >> 0 ) & 0x0001))
		sDigOutPorts[0]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_0 & 0x1F);
	else
		sDigOutPorts[0]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_0 & 0x1F);

	if((( pValue >> 1 ) & 0x0001))
		sDigOutPorts[1]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_1 & 0x1F);
	else
		sDigOutPorts[1]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_1 & 0x1F);

	if((( pValue >> 2 ) & 0x0001))
		sDigOutPorts[2]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_2 & 0x1F);
	else
		sDigOutPorts[2]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_2 & 0x1F);

	if((( pValue >> 3 ) & 0x0001))
		sDigOutPorts[3]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_3 & 0x1F);
	else
		sDigOutPorts[3]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_3 & 0x1F);

	if((( pValue >> 4 ) & 0x0001))
		sDigOutPorts[4]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_4 & 0x1F);
	else
		sDigOutPorts[4]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_4 & 0x1F);

	if((( pValue >> 5 ) & 0x0001))
		sDigOutPorts[5]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_5 & 0x1F);
	else
		sDigOutPorts[5]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_5 & 0x1F);

	if((( pValue >> 6 ) & 0x0001))
		sDigOutPorts[6]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_6 & 0x1F);
	else
		sDigOutPorts[6]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_6 & 0x1F);

	if((( pValue >> 7 ) & 0x0001))
		sDigOutPorts[7]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_7 & 0x1F);
	else
		sDigOutPorts[7]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_7 & 0x1F);

	if((( pValue >> 8 ) & 0x0001))
		sDigOutPorts[8]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_8 & 0x1F);
	else
		sDigOutPorts[8]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_8 & 0x1F);

	if((( pValue >> 9 ) & 0x0001))
		sDigOutPorts[9]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_9 & 0x1F);
	else
		sDigOutPorts[9]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_9 & 0x1F);

	if((( pValue >> 10 ) & 0x0001))
		sDigOutPorts[10]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_10 & 0x1F);
	else
		sDigOutPorts[10]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_10 & 0x1F);

	if((( pValue >> 11 ) & 0x0001))
		sDigOutPorts[11]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_11 & 0x1F);
	else
		sDigOutPorts[11]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_11 & 0x1F);

	if((( pValue >> 12 ) & 0x0001))
		sDigOutPorts[12]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_12 & 0x1F);
	else
		sDigOutPorts[12]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_12 & 0x1F);

	if((( pValue >> 13 ) & 0x0001))
		sDigOutPorts[13]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_13 & 0x1F);
	else
		sDigOutPorts[13]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_13 & 0x1F);

	if((( pValue >> 14 ) & 0x0001))
		sDigOutPorts[14]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_14 & 0x1F);
	else
		sDigOutPorts[14]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_14 & 0x1F);

	if((( pValue >> 15 ) & 0x0001))
		sDigOutPorts[15]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_15 & 0x1F);
	else
		sDigOutPorts[15]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_15 & 0x1F);
}

__attribute__((always_inline))
inline
void getDigOutValue(uint16_t * const pValue)
{
	uint16_t lValue = 0 ;
	lValue = lValue | ((sDigOutPorts[0]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_0 & 0x1F))) << 0) ;
	lValue = lValue | ((sDigOutPorts[1]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_1 & 0x1F))) << 1) ;
	lValue = lValue | ((sDigOutPorts[2]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_2 & 0x1F))) << 2) ;
	lValue = lValue | ((sDigOutPorts[3]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_3 & 0x1F))) << 3) ;
	lValue = lValue | ((sDigOutPorts[4]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_4 & 0x1F))) << 4) ;
	lValue = lValue | ((sDigOutPorts[5]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_5 & 0x1F))) << 5) ;
	lValue = lValue | ((sDigOutPorts[6]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_6 & 0x1F))) << 6) ;
	lValue = lValue | ((sDigOutPorts[7]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_7 & 0x1F))) << 7) ;
	lValue = lValue | ((sDigOutPorts[8]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_8 & 0x1F))) << 8) ;
	lValue = lValue | ((sDigOutPorts[9]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_9 & 0x1F))) << 9) ;
	lValue = lValue | ((sDigOutPorts[10]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_10 & 0x1F))) << 10) ;
	lValue = lValue | ((sDigOutPorts[11]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_11 & 0x1F))) << 11) ;
	lValue = lValue | ((sDigOutPorts[12]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_12 & 0x1F))) << 12) ;
	lValue = lValue | ((sDigOutPorts[13]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_13 & 0x1F))) << 13) ;
	lValue = lValue | ((sDigOutPorts[14]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_14 & 0x1F))) << 14) ;
	lValue = lValue | ((sDigOutPorts[15]->PIO_ODSR & (1 << (PHYCMD_DIGITAL_OUTPUT_15 & 0x1F))) << 15) ;
	*pValue = lValue ;
}

#define ADC_CHANNEL_NUM 8

volatile int bufn;
uint16_t buf[16][ADC_CHANNEL_NUM];   // 16 buffers of 8 readings

void ADC_Handler(){     // move DMA pointers to next buffer
	int f=ADC->ADC_ISR;
	if (f&(1<<27)){
		bufn=(bufn+1)&15;
		ADC->ADC_RNPR=(uint32_t)buf[bufn];
		ADC->ADC_RNCR=ADC_CHANNEL_NUM;
	}
}

void adc_setup(void)
{
	pmc_enable_periph_clk(ID_ADC);
	adc_init(ADC, sysclk_get_main_hz(), ADC_FREQ_MAX, ADC_STARTUP_FAST);

    adc_set_resolution(ADC, ADC_MR_LOWRES_BITS_12);

    adc_enable_channel(ADC, ADC_CHANNEL_0);
    adc_enable_channel(ADC, ADC_CHANNEL_1);
    adc_enable_channel(ADC, ADC_CHANNEL_2);
    adc_enable_channel(ADC, ADC_CHANNEL_3);
    adc_enable_channel(ADC, ADC_CHANNEL_4);
    adc_enable_channel(ADC, ADC_CHANNEL_5);
    adc_enable_channel(ADC, ADC_CHANNEL_6);
    adc_enable_channel(ADC, ADC_CHANNEL_7);
	
	ADC->ADC_MR |=0x80; // free running  
	ADC->ADC_CHER=0x80;
  
	ADC->ADC_IDR=~(1<<27);
	ADC->ADC_IER=1<<27;
	ADC->ADC_RPR=(uint32_t)buf[0];   // DMA buffer
	ADC->ADC_RCR=ADC_CHANNEL_NUM ; // number of readings
	ADC->ADC_RNPR=(uint32_t)buf[1]; // next DMA buffer
	ADC->ADC_RNCR=ADC_CHANNEL_NUM ; // number of readings
	bufn=1;
	ADC->ADC_PTCR=1;
	ADC->ADC_CR=2;	

	NVIC_EnableIRQ(ADC_IRQn);	
}

void dac_setup()
{
	pmc_enable_periph_clk(ID_DACC);	
	dacc_reset(DACC);           
	dacc_set_writeprotect(DACC, 0);
	dacc_set_transfer_mode(DACC, 1); 
	dacc_enable_flexible_selection(DACC);
	DACC->DACC_CHER = 3;  // enable channel 0 and 1

	dacc_set_timing(DACC, 0x01, 1, DACC_MR_STARTUP_0); // refresh - 0x01 (1024*1 dacc clocks), max speed mode - 1 (disabled), startup time   - 0x10 (1024 dacc clocks)
	dacc_set_analog_control(DACC, DACC_ACR_IBCTLCH0(0x02)|DACC_ACR_IBCTLCH1(0x02)|DACC_ACR_IBCTLDACCORE(0x01)); // power management
}

int main (void)
{
	sysclk_init();
	irq_initialize_vectors();
	cpu_irq_enable();
	board_init();

	udc_start();
	
	adc_setup();
	dac_setup();

	initDigInPorts(&sDigInPorts) ;
	initDigOutPorts(&sDigOutPorts);
			
	// Insert application code here, after the board has been initialized.
	bool lRes = false ;
	while (true)
	{
//		gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_0) ;
//		delay_us(1);
//		if (my_flag_autorize_cdc_transfert)
//		{
//			delay_us(10);
			if(udi_cdc_get_nb_received_data() == sSize)
			{
			//if(udi_cdc_read_buf(&sIn, sSize))
			//{				
//				gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_1) ;
///				sDigOutPorts[1]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_1 & 0x1F);
				udi_cdc_read_buf(&sIn, sSize) ;
///				sDigOutPorts[1]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_1 & 0x1F);
//				{
///				sDigOutPorts[2]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_2 & 0x1F);
//				for(size_t i = 0 ; i < sSize ; i++)
//				{
//					sOut[i] = sIn[i] ;
//				}
				memcpy(sOut,sIn,sSize);

				//execute commands				
				uint16_t lDigitalOut ;
				((uint8_t*)(&lDigitalOut))[0] = ((uint8_t*)(&sIn))[2];				
				((uint8_t*)(&lDigitalOut))[1] = ((uint8_t*)(&sIn))[3];
				setDigOutValue(lDigitalOut) ;

				//0000 0000 0000 0000
				// using full word writing
				uint16_t lDac0Out = ((uint16_t *)(&(sIn[4])))[0];
				uint16_t lDac1Out = ((uint16_t *)(&(sIn[6])))[0] | 0x1000 ;
				uint32_t lDacOut = lDac1Out <<16 | lDac0Out ;
				dacc_write_conversion_data(DACC, lDacOut) ;
				
				//prepare output packet								
				uint16_t lIn ;
				getDigInValue(&lIn) ;
				sOut[0] = ((uint8_t*)(&lIn))[0] ;				
				sOut[1] = ((uint8_t*)(&lIn))[1] ;

				getDigOutValue(lDigitalOut) ;
				sOut[2] = ((uint8_t*)(&lDigitalOut))[0] ;
				sOut[3] = ((uint8_t*)(&lDigitalOut))[1] ;
				
				memcpy(&(sOut[4]), buf[bufn], sizeof(uint16_t) * ADC_CHANNEL_NUM) ;

//				if(udi_cdc_get_free_tx_buffer() >= sSize)
//				{
///				sDigOutPorts[2]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_2 & 0x1F);
///				sDigOutPorts[0]->PIO_SODR = 1 << (PHYCMD_DIGITAL_OUTPUT_0 & 0x1F);	
				udi_cdc_write_buf(&sOut, sSize);
				
				
///				sDigOutPorts[0]->PIO_CODR = 1 << (PHYCMD_DIGITAL_OUTPUT_0 & 0x1F);
//				}
//				}
//				gpio_set_pin_low(PHYCMD_DIGITAL_OUTPUT_1) ;
			}
//		}
//		gpio_set_pin_high(PHYCMD_DIGITAL_OUTPUT_0) ;
	}
}