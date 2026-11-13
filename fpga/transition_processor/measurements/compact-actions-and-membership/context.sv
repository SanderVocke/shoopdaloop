module registered_core_context #(

    parameter integer LOOP_DEPTH = 64,
    parameter integer REQUEST_DEPTH = 64,
    parameter integer EVENT_DEPTH = 192,
    parameter integer INTEGRATED = 0,
    parameter integer HAS_EXEC_INTERFACE = 1

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
localparam integer PORT_BITS_command_valid = 1;
localparam integer SOURCE_BITS_command_valid = PORT_BITS_command_valid < 32 ? 32 : PORT_BITS_command_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_valid-1:0] source_command_valid = SOURCE_BITS_command_valid'(3);
always @(posedge clk) source_command_valid <= {source_command_valid[SOURCE_BITS_command_valid-2:0],source_command_valid[SOURCE_BITS_command_valid-1]^source_command_valid[SOURCE_BITS_command_valid-2]};
wire  command_valid = PORT_BITS_command_valid'(source_command_valid);
wire  command_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_command_ready;
always @(posedge clk) unused_capture_command_ready <= command_ready;
localparam integer PORT_BITS_command_opcode = (7)+1;
localparam integer SOURCE_BITS_command_opcode = PORT_BITS_command_opcode < 32 ? 32 : PORT_BITS_command_opcode;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_opcode-1:0] source_command_opcode = SOURCE_BITS_command_opcode'(5);
always @(posedge clk) source_command_opcode <= {source_command_opcode[SOURCE_BITS_command_opcode-2:0],source_command_opcode[SOURCE_BITS_command_opcode-1]^source_command_opcode[SOURCE_BITS_command_opcode-2]};
wire [7:0] command_opcode = PORT_BITS_command_opcode'(source_command_opcode);
localparam integer PORT_BITS_command_index = (31)+1;
localparam integer SOURCE_BITS_command_index = PORT_BITS_command_index < 32 ? 32 : PORT_BITS_command_index;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_index-1:0] source_command_index = SOURCE_BITS_command_index'(6);
always @(posedge clk) source_command_index <= {source_command_index[SOURCE_BITS_command_index-2:0],source_command_index[SOURCE_BITS_command_index-1]^source_command_index[SOURCE_BITS_command_index-2]};
wire [31:0] command_index = PORT_BITS_command_index'(source_command_index);
localparam integer PORT_BITS_command_data = (639)+1;
localparam integer SOURCE_BITS_command_data = PORT_BITS_command_data < 32 ? 32 : PORT_BITS_command_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_data-1:0] source_command_data = SOURCE_BITS_command_data'(7);
always @(posedge clk) source_command_data <= {source_command_data[SOURCE_BITS_command_data-2:0],source_command_data[SOURCE_BITS_command_data-1]^source_command_data[SOURCE_BITS_command_data-2]};
wire [639:0] command_data = PORT_BITS_command_data'(source_command_data);
wire  response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_response_valid;
always @(posedge clk) unused_capture_response_valid <= response_valid;
localparam integer PORT_BITS_response_ready = 1;
localparam integer SOURCE_BITS_response_ready = PORT_BITS_response_ready < 32 ? 32 : PORT_BITS_response_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_response_ready-1:0] source_response_ready = SOURCE_BITS_response_ready'(9);
always @(posedge clk) source_response_ready <= {source_response_ready[SOURCE_BITS_response_ready-2:0],source_response_ready[SOURCE_BITS_response_ready-1]^source_response_ready[SOURCE_BITS_response_ready-2]};
wire  response_ready = PORT_BITS_response_ready'(source_response_ready);
wire [3:0] response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] unused_capture_response_error;
always @(posedge clk) unused_capture_response_error <= response_error;
wire [831:0] response_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [831:0] unused_capture_response_data;
always @(posedge clk) unused_capture_response_data <= response_data;
wire  busy;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_busy;
always @(posedge clk) unused_capture_busy <= busy;
localparam integer PORT_BITS_loop_read_valid = 1;
localparam integer SOURCE_BITS_loop_read_valid = PORT_BITS_loop_read_valid < 32 ? 32 : PORT_BITS_loop_read_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_read_valid-1:0] source_loop_read_valid = SOURCE_BITS_loop_read_valid'(13);
always @(posedge clk) source_loop_read_valid <= {source_loop_read_valid[SOURCE_BITS_loop_read_valid-2:0],source_loop_read_valid[SOURCE_BITS_loop_read_valid-1]^source_loop_read_valid[SOURCE_BITS_loop_read_valid-2]};
wire  loop_read_valid = PORT_BITS_loop_read_valid'(source_loop_read_valid);
wire  loop_read_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_loop_read_ready;
always @(posedge clk) unused_capture_loop_read_ready <= loop_read_ready;
localparam integer PORT_BITS_loop_read_slot = (31)+1;
localparam integer SOURCE_BITS_loop_read_slot = PORT_BITS_loop_read_slot < 32 ? 32 : PORT_BITS_loop_read_slot;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_loop_read_slot-1:0] source_loop_read_slot = SOURCE_BITS_loop_read_slot'(15);
always @(posedge clk) source_loop_read_slot <= {source_loop_read_slot[SOURCE_BITS_loop_read_slot-2:0],source_loop_read_slot[SOURCE_BITS_loop_read_slot-1]^source_loop_read_slot[SOURCE_BITS_loop_read_slot-2]};
wire [31:0] loop_read_slot = PORT_BITS_loop_read_slot'(source_loop_read_slot);
wire  loop_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_loop_response_valid;
always @(posedge clk) unused_capture_loop_response_valid <= loop_response_valid;
wire  loop_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_loop_response_error;
always @(posedge clk) unused_capture_loop_response_error <= loop_response_error;
wire [163:0] loop_response_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [163:0] unused_capture_loop_response_data;
always @(posedge clk) unused_capture_loop_response_data <= loop_response_data;
wire [31:0] configured_quantum;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] unused_capture_configured_quantum;
always @(posedge clk) unused_capture_configured_quantum <= configured_quantum;
localparam integer PORT_BITS_execute_valid = 1;
localparam integer SOURCE_BITS_execute_valid = PORT_BITS_execute_valid < 32 ? 32 : PORT_BITS_execute_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_execute_valid-1:0] source_execute_valid = SOURCE_BITS_execute_valid'(20);
always @(posedge clk) source_execute_valid <= {source_execute_valid[SOURCE_BITS_execute_valid-2:0],source_execute_valid[SOURCE_BITS_execute_valid-1]^source_execute_valid[SOURCE_BITS_execute_valid-2]};
wire  execute_valid = PORT_BITS_execute_valid'(source_execute_valid);
wire  execute_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_execute_ready;
always @(posedge clk) unused_capture_execute_ready <= execute_ready;
localparam integer PORT_BITS_execute_finish = 1;
localparam integer SOURCE_BITS_execute_finish = PORT_BITS_execute_finish < 32 ? 32 : PORT_BITS_execute_finish;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_execute_finish-1:0] source_execute_finish = SOURCE_BITS_execute_finish'(22);
always @(posedge clk) source_execute_finish <= {source_execute_finish[SOURCE_BITS_execute_finish-2:0],source_execute_finish[SOURCE_BITS_execute_finish-1]^source_execute_finish[SOURCE_BITS_execute_finish-2]};
wire  execute_finish = PORT_BITS_execute_finish'(source_execute_finish);
wire  execute_done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  unused_capture_execute_done;
always @(posedge clk) unused_capture_execute_done <= execute_done;
wire [3:0] execute_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] unused_capture_execute_error;
always @(posedge clk) unused_capture_execute_error <= execute_error;
(* KEEP_HIERARCHY = "yes" *) transition_measure_adapter #(
.LOOP_DEPTH(LOOP_DEPTH),
.REQUEST_DEPTH(REQUEST_DEPTH),
.EVENT_DEPTH(EVENT_DEPTH),
.INTEGRATED(INTEGRATED),
.HAS_EXEC_INTERFACE(HAS_EXEC_INTERFACE)
) core_under_test (
.clk(clk),
.reset(reset),
.command_valid(command_valid),
.command_ready(command_ready),
.command_opcode(command_opcode),
.command_index(command_index),
.command_data(command_data),
.response_valid(response_valid),
.response_ready(response_ready),
.response_error(response_error),
.response_data(response_data),
.busy(busy),
.loop_read_valid(loop_read_valid),
.loop_read_ready(loop_read_ready),
.loop_read_slot(loop_read_slot),
.loop_response_valid(loop_response_valid),
.loop_response_error(loop_response_error),
.loop_response_data(loop_response_data),
.configured_quantum(configured_quantum),
.execute_valid(execute_valid),
.execute_ready(execute_ready),
.execute_finish(execute_finish),
.execute_done(execute_done),
.execute_error(execute_error)
);
endmodule
