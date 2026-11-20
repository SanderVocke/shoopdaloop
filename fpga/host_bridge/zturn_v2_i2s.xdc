# MYIR Z-turn V2 Ver 1 schematic, J11 and fixed 3.3 V bank 13.
set_property -dict {PACKAGE_PIN U7 IOSTANDARD LVCMOS33} [get_ports audio_ref]
set_property -dict {PACKAGE_PIN T9 IOSTANDARD LVCMOS33} [get_ports i2s_bclk]
set_property -dict {PACKAGE_PIN U10 IOSTANDARD LVCMOS33} [get_ports i2s_lrclk]
set_property -dict {PACKAGE_PIN Y7 IOSTANDARD LVCMOS33} [get_ports i2s_dac_data]
set_property -dict {PACKAGE_PIN Y6 IOSTANDARD LVCMOS33} [get_ports i2s_adc_data]
set_property -dict {DRIVE 8 SLEW SLOW} [get_ports {i2s_bclk i2s_lrclk i2s_dac_data}]
create_clock -name audio_ref -period 81.380 [get_ports audio_ref]
create_generated_clock -name codec_sclk -source [get_ports audio_ref] -divide_by 4 [get_ports i2s_bclk]
# Conservative adapter budgets in ns, including the short connection to the Pmod.
set_input_delay -clock codec_sclk -clock_fall -max 50 [get_ports i2s_adc_data]
set_input_delay -clock codec_sclk -clock_fall -min 0 [get_ports i2s_adc_data]
set_output_delay -clock codec_sclk -max 25 [get_ports {i2s_lrclk i2s_dac_data}]
set_output_delay -clock codec_sclk -min -25 [get_ports {i2s_lrclk i2s_dac_data}]
