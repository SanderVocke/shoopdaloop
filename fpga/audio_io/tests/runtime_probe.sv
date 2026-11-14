module runtime_probe (
    input wire clk,reset,loaded,input wire [63:0] graph_epoch,
    input wire request_valid,output wire request_ready,input wire [7:0] request_opcode,input wire request_state,
    input wire [15:0] request_address,input wire [639:0] request_data,
    output wire response_valid,output wire [3:0] response_error,output wire [128:0] response_data,
    output wire fault,image_busy,input wire run_valid,output wire run_ready,busy,done,output wire [7:0] error_code,
    input wire job_ready,output wire job_valid,output wire [255:0] job_data,
    input wire control_valid,output wire control_ready,input wire [7:0] control_opcode,input wire [639:0] control_data,
    output wire control_done,output wire [3:0] control_error,output wire [128:0] control_result,
    input wire runtime_valid,output wire runtime_ready,input wire [511:0] runtime_data,
    output wire runtime_done,output wire [511:0] runtime_result,output wire runtime_fault,
    input wire observe_valid,output wire observe_ready,input wire [831:0] observe_data,input wire pre_recording,output wire observe_done,
    input wire activation_valid,output wire activation_ready,input wire activation_initial,activation_stereo,
    input wire [63:0] activation_epoch,output wire activation_done,
    output wire [31:0] loop_gain,loop_balance,smoothing_frames,output wire smoothing_linear
);
    wire owner_busy,owner_parameter_valid,owner_parameter_ready;
    wire [511:0] owner_parameter_data;
    wire manager_ready,manager_busy,manager_done,manager_fault;
    wire [3:0] manager_error;
    wire [128:0] manager_result;
    wire metadata_ready,metadata_done;
    wire [3:0] metadata_error;
    wire [128:0] metadata_result;
    wire native_ready,observation_ready,initial_ready;
    wire private_parameter=control_opcode == 49;
    wire quiet=!owner_busy && !runtime_valid && !observe_valid && !activation_valid;
    assign control_ready=private_parameter ? manager_ready && quiet : metadata_ready && !manager_busy;
    assign control_done=private_parameter ? manager_done : metadata_done;
    assign control_error=private_parameter ? manager_error : metadata_error;
    assign control_result=private_parameter ? manager_result : metadata_result;
    assign runtime_ready=native_ready && !manager_busy;
    assign observe_ready=observation_ready && !manager_busy;
    assign activation_ready=initial_ready && !manager_busy;
    wire [511:0] command=owner_busy ? owner_parameter_data : control_data[511:0];
    wire unused=&{1'b0,manager_fault,control_data[631:512],command[511:337],command[31:25]};
    audio_runtime #(.NODES(8)) runtime (
        .clk(clk),.reset(reset),.loaded(loaded),.graph_epoch(graph_epoch),
        .configuration_valid(control_valid && !private_parameter && !manager_busy),.configuration_ready(metadata_ready),
        .configuration_data(control_data),.configuration_done(metadata_done),.configuration_error(metadata_error),.configuration_result(metadata_result),
        .request_valid(runtime_valid && !manager_busy),.request_ready(native_ready),.request_data(runtime_data),
        .response_valid(runtime_done),.response_data(runtime_result),
        .observe_valid(observe_valid && !manager_busy),.observe_ready(observation_ready),.observe_data(observe_data),.pre_recording(pre_recording),.observe_done(observe_done),
        .activation_valid(activation_valid && !manager_busy),.activation_ready(initial_ready),.activation_initial(activation_initial),
        .activation_stereo(activation_stereo),.activation_epoch(activation_epoch),.activation_done(activation_done),
        .parameter_valid(owner_parameter_valid),.parameter_ready(owner_parameter_ready),.parameter_data(owner_parameter_data),
        .parameter_done(manager_done),.parameter_error(manager_error),.parameter_result(manager_result),
        .loop_gain(loop_gain),.loop_balance(loop_balance),.busy(owner_busy),.fault(runtime_fault)
    );
    assign owner_parameter_ready=manager_ready && owner_busy;
    parameters_probe #(.KEYS(56)) image (
        .clk(clk),.reset(reset),.initialization_open(!loaded),
        .request_valid(request_valid),.request_ready(request_ready),.request_opcode(request_opcode),.request_state(request_state),
        .request_address(request_address),.request_data(request_data),.response_valid(response_valid),.response_error(response_error),.response_data(response_data),
        .fault(fault),.image_busy(image_busy),.run_valid(run_valid),.run_ready(run_ready),.busy(busy),.done(done),.error_code(error_code),
        .job_ready(job_ready),.job_valid(job_valid),.job_data(job_data),
        .parameter_valid(owner_parameter_valid || (control_valid && private_parameter && quiet)),.parameter_ready(manager_ready),
        .parameter_opcode(command[7:0]),.parameter_key(command[23:8]),.parameter_bank(command[24]),.parameter_nonce(command[95:32]),
        .parameter_count(command[127:96]),.parameter_address(command[143:128]),.parameter_value(command[272:144]),
        .parameter_target(command[304:273]),.parameter_duration(command[336:305]),.parameter_busy(manager_busy),
        .parameter_response_valid(manager_done),.parameter_response_error(manager_error),.parameter_response_data(manager_result),.parameter_fault(manager_fault),
        .smoothing_frames(smoothing_frames),.smoothing_linear(smoothing_linear)
    );
endmodule
