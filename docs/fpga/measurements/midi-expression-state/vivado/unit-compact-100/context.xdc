create_clock -name core -period 10.000000 [get_ports clk]
set_clock_uncertainty 0.200 [get_clocks core]
set_property HD.CLK_SRC BUFGCTRL_X0Y0 [get_ports clk]
