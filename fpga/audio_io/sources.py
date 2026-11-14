"""Source manifests for the real continuous processing and demonstration tops."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def sources(demo=False):
    groups = [
        ('state_processor', ['sp_ram', 'sp_job_fifo', 'sp_engine', 'state_processor']),
        ('job_processor', ['jp_sample_ram', 'jp_divider', 'job_processor', 'audio_pipeline']),
        ('transition_processor', ['tp_ram', 'tp_priority', 'transition_processor']),
        ('chunk_memory', ['chunk_ram', 'chunk_memory']),
        ('media_store', ['media_ram', 'metadata_cache', 'media_metadata', 'media_reclaimer', 'media_prefetch_resolver',
                         'media_store', 'memory_delay', 'media_audio_pipeline']),
        ('loop_pipeline', ['media_cuts', 'cut_records', 'native_requests', 'loop_pipeline']),
        ('axi_memory', ['axi_master', 'axi_burst_mux', 'shared_axi', 'chunk_dma', 'word_dma',
                        'word_axi', 'chunk_axi', 'loop_pipeline_axi']),
        ('audio_io', ['audio_lane_ram', 'audio_io', 'audio_command_fifo', 'audio_image', 'audio_parameters', 'audio_stages', 'audio_activation', 'audio_configuration', 'audio_actions', 'audio_native', 'audio_observations', 'audio_control', 'audio_loop_pipeline'])]
    if demo:
        groups.append(('audio_io', ['audio_boot', 'audio_events', 'audio_demo']))
    return [ROOT/'fpga'/directory/'rtl'/f'{name}.sv' for directory, names in groups for name in names]
