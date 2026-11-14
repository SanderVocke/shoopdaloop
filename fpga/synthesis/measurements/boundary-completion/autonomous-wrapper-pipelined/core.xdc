create_clock -name core_clk -period 10.000 [get_ports clk]
set_clock_uncertainty 0.200 [get_clocks core_clk]
set_property HD.CLK_SRC BUFGCTRL_X0Y0 [get_ports clk]

create_clock -name audio_clk -period 325.520833 [get_ports audio_clk]
set_clock_uncertainty 0.200 [get_clocks audio_clk]
set_clock_groups -asynchronous -group [get_clocks core_clk] -group [get_clocks audio_clk]
