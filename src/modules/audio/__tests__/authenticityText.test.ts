import { describe, expect, it } from "vitest";
import type { AuthenticityReport } from "@domain/audio";
import { reportToText } from "../AuthenticityView";

const base: AuthenticityReport = {
  source_kind: "original importado",
  source_file: "x.wav",
  structure: {
    format: "wav",
    format_long: "WAV / WAVE",
    duration_s: 11.05,
    bit_rate: 768000,
    format_tags: { encoder: "Lavf63.1.101" },
    codec: "pcm_s16le",
    codec_long: "PCM 16-bit",
    profile: null,
    sample_rate: 48000,
    channels: 1,
    channel_layout: null,
    bits_per_sample: 16,
    stream_bit_rate: 768000,
    start_time_s: 0,
    stream_tags: {},
    lossless: true,
  },
  packets: { count: 130, gaps: [], overlaps: 0, size_min: 4032, size_max: 8192, size_mode: "variável" },
  decode_errors: 0,
  decode_error_samples: [],
  bandwidth: { nyquist_hz: 24000, cutoff_hz: 16012.5, drop_db: 41.2, steep: true, segments: [], varies: false },
  clicks: [],
  clicks_total: 0,
  digital_silences: [[4, 0.05]],
  digital_silences_total: 1,
  noise_jumps: [{ t_s: 7.25, delta_db: 30.4, band: "muito agudo" }],
  notes: ["nota"],
};

describe("reportToText", () => {
  it("texto para o laudo com vírgula decimal e os achados", () => {
    const t = reportToText(base);
    expect(t).toContain("duração 11,050 s");
    expect(t).toContain("em degrau em 16,0 kHz (queda de 41 dB");
    expect(t).toContain("0:04,00 por 50 ms");
    expect(t).toContain("0:07,25 +30 dB no muito agudo");
    expect(t).toContain("encoder=Lavf63.1.101");
    expect(t).toMatch(/não concluem sobre edição/);
  });
});
