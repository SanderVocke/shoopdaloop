module expression_context #(

    parameter integer LOOPS=128, INPUTS=1, DESTINATIONS=8, VIEWS=512

) (input wire clk);
reg [7:0] startup = 0;
always @(posedge clk) if (!(&startup)) startup <= startup+1'b1;
reg reset = 1;
always @(posedge clk) reset <= !(&startup);
wire [63:0] arena_base = 64'd0;
wire [63:0] arena_bytes;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_arena_bytes;
always @(posedge clk) capture_arena_bytes <= arena_bytes;
localparam integer PORT_BITS_command_valid = 1;
localparam integer SOURCE_BITS_command_valid = PORT_BITS_command_valid < 32 ? 32 : PORT_BITS_command_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_valid-1:0] source_command_valid = SOURCE_BITS_command_valid'(5);
always @(posedge clk) source_command_valid <= {source_command_valid[SOURCE_BITS_command_valid-2:0],source_command_valid[SOURCE_BITS_command_valid-1]^source_command_valid[SOURCE_BITS_command_valid-2]};
wire  command_valid = PORT_BITS_command_valid'(source_command_valid);
wire  command_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_command_ready;
always @(posedge clk) capture_command_ready <= command_ready;
localparam integer PORT_BITS_command_op = (3)+1;
localparam integer SOURCE_BITS_command_op = PORT_BITS_command_op < 32 ? 32 : PORT_BITS_command_op;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_command_op-1:0] source_command_op = SOURCE_BITS_command_op'(7);
always @(posedge clk) source_command_op <= {source_command_op[SOURCE_BITS_command_op-2:0],source_command_op[SOURCE_BITS_command_op-1]^source_command_op[SOURCE_BITS_command_op-2]};
wire [3:0] command_op = PORT_BITS_command_op'(source_command_op);
localparam integer PORT_BITS_arg0 = (63)+1;
localparam integer SOURCE_BITS_arg0 = PORT_BITS_arg0 < 32 ? 32 : PORT_BITS_arg0;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_arg0-1:0] source_arg0 = SOURCE_BITS_arg0'(8);
always @(posedge clk) source_arg0 <= {source_arg0[SOURCE_BITS_arg0-2:0],source_arg0[SOURCE_BITS_arg0-1]^source_arg0[SOURCE_BITS_arg0-2]};
wire [63:0] arg0 = PORT_BITS_arg0'(source_arg0);
localparam integer PORT_BITS_arg1 = (63)+1;
localparam integer SOURCE_BITS_arg1 = PORT_BITS_arg1 < 32 ? 32 : PORT_BITS_arg1;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_arg1-1:0] source_arg1 = SOURCE_BITS_arg1'(9);
always @(posedge clk) source_arg1 <= {source_arg1[SOURCE_BITS_arg1-2:0],source_arg1[SOURCE_BITS_arg1-1]^source_arg1[SOURCE_BITS_arg1-2]};
wire [63:0] arg1 = PORT_BITS_arg1'(source_arg1);
localparam integer PORT_BITS_arg2 = (63)+1;
localparam integer SOURCE_BITS_arg2 = PORT_BITS_arg2 < 32 ? 32 : PORT_BITS_arg2;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_arg2-1:0] source_arg2 = SOURCE_BITS_arg2'(10);
always @(posedge clk) source_arg2 <= {source_arg2[SOURCE_BITS_arg2-2:0],source_arg2[SOURCE_BITS_arg2-1]^source_arg2[SOURCE_BITS_arg2-2]};
wire [63:0] arg2 = PORT_BITS_arg2'(source_arg2);
localparam integer PORT_BITS_arg3 = (63)+1;
localparam integer SOURCE_BITS_arg3 = PORT_BITS_arg3 < 32 ? 32 : PORT_BITS_arg3;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_arg3-1:0] source_arg3 = SOURCE_BITS_arg3'(11);
always @(posedge clk) source_arg3 <= {source_arg3[SOURCE_BITS_arg3-2:0],source_arg3[SOURCE_BITS_arg3-1]^source_arg3[SOURCE_BITS_arg3-2]};
wire [63:0] arg3 = PORT_BITS_arg3'(source_arg3);
wire  tracking_enabled;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_tracking_enabled;
always @(posedge clk) capture_tracking_enabled <= tracking_enabled;
wire  done;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_done;
always @(posedge clk) capture_done <= done;
wire [3:0] error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [3:0] capture_error;
always @(posedge clk) capture_error <= error;
wire [31:0] result;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_result;
always @(posedge clk) capture_result <= result;
wire  event_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_event_valid;
always @(posedge clk) capture_event_valid <= event_valid;
localparam integer PORT_BITS_event_ready = 1;
localparam integer SOURCE_BITS_event_ready = PORT_BITS_event_ready < 32 ? 32 : PORT_BITS_event_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_event_ready-1:0] source_event_ready = SOURCE_BITS_event_ready'(17);
always @(posedge clk) source_event_ready <= {source_event_ready[SOURCE_BITS_event_ready-2:0],source_event_ready[SOURCE_BITS_event_ready-1]^source_event_ready[SOURCE_BITS_event_ready-2]};
wire  event_ready = PORT_BITS_event_ready'(source_event_ready);
wire [63:0] event_view;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_event_view;
always @(posedge clk) capture_event_view <= event_view;
wire [31:0] event_index;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_event_index;
always @(posedge clk) capture_event_index <= event_index;
localparam integer PORT_BITS_event_response_valid = 1;
localparam integer SOURCE_BITS_event_response_valid = PORT_BITS_event_response_valid < 32 ? 32 : PORT_BITS_event_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_event_response_valid-1:0] source_event_response_valid = SOURCE_BITS_event_response_valid'(20);
always @(posedge clk) source_event_response_valid <= {source_event_response_valid[SOURCE_BITS_event_response_valid-2:0],source_event_response_valid[SOURCE_BITS_event_response_valid-1]^source_event_response_valid[SOURCE_BITS_event_response_valid-2]};
wire  event_response_valid = PORT_BITS_event_response_valid'(source_event_response_valid);
localparam integer PORT_BITS_event_error = (3)+1;
localparam integer SOURCE_BITS_event_error = PORT_BITS_event_error < 32 ? 32 : PORT_BITS_event_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_event_error-1:0] source_event_error = SOURCE_BITS_event_error'(21);
always @(posedge clk) source_event_error <= {source_event_error[SOURCE_BITS_event_error-2:0],source_event_error[SOURCE_BITS_event_error-1]^source_event_error[SOURCE_BITS_event_error-2]};
wire [3:0] event_error = PORT_BITS_event_error'(source_event_error);
localparam integer PORT_BITS_event_frame = (63)+1;
localparam integer SOURCE_BITS_event_frame = PORT_BITS_event_frame < 32 ? 32 : PORT_BITS_event_frame;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_event_frame-1:0] source_event_frame = SOURCE_BITS_event_frame'(22);
always @(posedge clk) source_event_frame <= {source_event_frame[SOURCE_BITS_event_frame-2:0],source_event_frame[SOURCE_BITS_event_frame-1]^source_event_frame[SOURCE_BITS_event_frame-2]};
wire [63:0] event_frame = PORT_BITS_event_frame'(source_event_frame);
localparam integer PORT_BITS_event_packet = (31)+1;
localparam integer SOURCE_BITS_event_packet = PORT_BITS_event_packet < 32 ? 32 : PORT_BITS_event_packet;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_event_packet-1:0] source_event_packet = SOURCE_BITS_event_packet'(23);
always @(posedge clk) source_event_packet <= {source_event_packet[SOURCE_BITS_event_packet-2:0],source_event_packet[SOURCE_BITS_event_packet-1]^source_event_packet[SOURCE_BITS_event_packet-2]};
wire [31:0] event_packet = PORT_BITS_event_packet'(source_event_packet);
wire  packet_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_packet_valid;
always @(posedge clk) capture_packet_valid <= packet_valid;
localparam integer PORT_BITS_packet_ready = 1;
localparam integer SOURCE_BITS_packet_ready = PORT_BITS_packet_ready < 32 ? 32 : PORT_BITS_packet_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_packet_ready-1:0] source_packet_ready = SOURCE_BITS_packet_ready'(25);
always @(posedge clk) source_packet_ready <= {source_packet_ready[SOURCE_BITS_packet_ready-2:0],source_packet_ready[SOURCE_BITS_packet_ready-1]^source_packet_ready[SOURCE_BITS_packet_ready-2]};
wire  packet_ready = PORT_BITS_packet_ready'(source_packet_ready);
localparam integer PORT_BITS_packet_refused = 1;
localparam integer SOURCE_BITS_packet_refused = PORT_BITS_packet_refused < 32 ? 32 : PORT_BITS_packet_refused;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_packet_refused-1:0] source_packet_refused = SOURCE_BITS_packet_refused'(26);
always @(posedge clk) source_packet_refused <= {source_packet_refused[SOURCE_BITS_packet_refused-2:0],source_packet_refused[SOURCE_BITS_packet_refused-1]^source_packet_refused[SOURCE_BITS_packet_refused-2]};
wire  packet_refused = PORT_BITS_packet_refused'(source_packet_refused);
wire [31:0] packet;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_packet;
always @(posedge clk) capture_packet <= packet;
wire [15:0] destination;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [15:0] capture_destination;
always @(posedge clk) capture_destination <= destination;
wire  memory_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_memory_valid;
always @(posedge clk) capture_memory_valid <= memory_valid;
localparam integer PORT_BITS_memory_ready = 1;
localparam integer SOURCE_BITS_memory_ready = PORT_BITS_memory_ready < 32 ? 32 : PORT_BITS_memory_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_memory_ready-1:0] source_memory_ready = SOURCE_BITS_memory_ready'(30);
always @(posedge clk) source_memory_ready <= {source_memory_ready[SOURCE_BITS_memory_ready-2:0],source_memory_ready[SOURCE_BITS_memory_ready-1]^source_memory_ready[SOURCE_BITS_memory_ready-2]};
wire  memory_ready = PORT_BITS_memory_ready'(source_memory_ready);
wire  memory_write;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_memory_write;
always @(posedge clk) capture_memory_write <= memory_write;
wire [63:0] memory_address;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [63:0] capture_memory_address;
always @(posedge clk) capture_memory_address <= memory_address;
wire [31:0] memory_write_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [31:0] capture_memory_write_data;
always @(posedge clk) capture_memory_write_data <= memory_write_data;
localparam integer PORT_BITS_memory_response_valid = 1;
localparam integer SOURCE_BITS_memory_response_valid = PORT_BITS_memory_response_valid < 32 ? 32 : PORT_BITS_memory_response_valid;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_memory_response_valid-1:0] source_memory_response_valid = SOURCE_BITS_memory_response_valid'(34);
always @(posedge clk) source_memory_response_valid <= {source_memory_response_valid[SOURCE_BITS_memory_response_valid-2:0],source_memory_response_valid[SOURCE_BITS_memory_response_valid-1]^source_memory_response_valid[SOURCE_BITS_memory_response_valid-2]};
wire  memory_response_valid = PORT_BITS_memory_response_valid'(source_memory_response_valid);
wire  memory_response_ready;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg  capture_memory_response_ready;
always @(posedge clk) capture_memory_response_ready <= memory_response_ready;
localparam integer PORT_BITS_memory_response_error = 1;
localparam integer SOURCE_BITS_memory_response_error = PORT_BITS_memory_response_error < 32 ? 32 : PORT_BITS_memory_response_error;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_memory_response_error-1:0] source_memory_response_error = SOURCE_BITS_memory_response_error'(36);
always @(posedge clk) source_memory_response_error <= {source_memory_response_error[SOURCE_BITS_memory_response_error-2:0],source_memory_response_error[SOURCE_BITS_memory_response_error-1]^source_memory_response_error[SOURCE_BITS_memory_response_error-2]};
wire  memory_response_error = PORT_BITS_memory_response_error'(source_memory_response_error);
localparam integer PORT_BITS_memory_read_data = (31)+1;
localparam integer SOURCE_BITS_memory_read_data = PORT_BITS_memory_read_data < 32 ? 32 : PORT_BITS_memory_read_data;
(* DONT_TOUCH = "yes", SHREG_EXTRACT = "no" *) reg [SOURCE_BITS_memory_read_data-1:0] source_memory_read_data = SOURCE_BITS_memory_read_data'(37);
always @(posedge clk) source_memory_read_data <= {source_memory_read_data[SOURCE_BITS_memory_read_data-2:0],source_memory_read_data[SOURCE_BITS_memory_read_data-1]^source_memory_read_data[SOURCE_BITS_memory_read_data-2]};
wire [31:0] memory_read_data = PORT_BITS_memory_read_data'(source_memory_read_data);
(* KEEP_HIERARCHY = "yes" *) midi_expression_state #(
.LOOPS(LOOPS),
.INPUTS(INPUTS),
.DESTINATIONS(DESTINATIONS),
.VIEWS(VIEWS)
) core_under_test (
.clk(clk),
.reset(reset),
.arena_base(arena_base),
.arena_bytes(arena_bytes),
.command_valid(command_valid),
.command_ready(command_ready),
.command_op(command_op),
.arg0(arg0),
.arg1(arg1),
.arg2(arg2),
.arg3(arg3),
.tracking_enabled(tracking_enabled),
.done(done),
.error(error),
.result(result),
.event_valid(event_valid),
.event_ready(event_ready),
.event_view(event_view),
.event_index(event_index),
.event_response_valid(event_response_valid),
.event_error(event_error),
.event_frame(event_frame),
.event_packet(event_packet),
.packet_valid(packet_valid),
.packet_ready(packet_ready),
.packet_refused(packet_refused),
.packet(packet),
.destination(destination),
.memory_valid(memory_valid),
.memory_ready(memory_ready),
.memory_write(memory_write),
.memory_address(memory_address),
.memory_write_data(memory_write_data),
.memory_response_valid(memory_response_valid),
.memory_response_ready(memory_response_ready),
.memory_response_error(memory_response_error),
.memory_read_data(memory_read_data)
);
endmodule
module session_context(input wire clk);
expression_context #(.LOOPS(64),.INPUTS(1),.DESTINATIONS(8),.VIEWS(256)) expression (.clk(clk));
endmodule
