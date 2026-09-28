# Alternatives and complements to physical engine simulation, and the realism layers a gas-dynamics model still needs

Scope: neural/DDSP engine synthesis (2023–2026), differentiable physical models, hybrid physics + learned/measured, post-gas-model realism layers (mechanical noise, structure/radiation, cabin/exterior/environment IRs), free IR availability/licensing, and commercial/pro tools. Researched 2026-09-28. Throughout, "arbitrary designed engine" = an engine a user invents in a tool (no recording exists); "needs recordings" = the method must be fitted to audio of that specific engine.

## Q1 — Neural and DDSP engine sound synthesis, 2023–2026

### Takeaway
Every neural/DDSP engine synthesizer found (PTR, EONE, Motor2Synth, Li/Wang/Li, CAN-bus DDSP) is conditioned on rpm plus torque/pedal and is trained or fine-tuned on recordings of the target engine (or on procedural data derived from recordings). None has been shown to generate a convincing sound for an unrecorded, user-designed engine from geometry alone. The best reported realism comes from EONE (Doerfler/Kuntz/Zimmer 2026). Naive listeners rated its resynthesis the same as the recording (61 vs 61). Experts could still tell them apart (71 vs 76, p<0.03). That result depends on fine-tuning to the target recordings.

### Cited Findings
**Doerfler & Wyse — Pulse-Train-Resonator (PTR), arXiv 2603.09391 (2026)**
- Architecture: parameterized pulse trains locked to engine firing cycles with pressure-release envelopes, fed into recursive differentiable Karplus-Strong resonators that model exhaust acoustics. Physics-informed biases include harmonic decay, thermodynamic pitch modulation, valve-dynamics envelopes and exhaust resonances. — [arXiv 2603.09391](https://arxiv.org/abs/2603.09391)
- Inputs: {RPM, Nm, ΔRPM, ΔNm, Δ²RPM, Δ²Nm} at a 125 Hz frame rate. A physics gate g_thr = max(torque, ε)^0.7 switches on combustion noise, and a DFCO gate g_DFCO = max(−torque, ε) switches on deceleration fuel-cut airflow noise. Network: MLP, then GRU, then MLP parameter decoder. Audio at 16 kHz in 4 s chunks. — [arXiv 2603.09391 HTML](https://arxiv.org/html/2603.09391v1)
- Data: three procedural datasets of about 2.5 h each, 7.5 h total (A = inline-4; B and C = V8s with different exhaust resonances). One unified model covers all three. — [arXiv 2603.09391 HTML](https://arxiv.org/html/2603.09391v1)
- Results versus a harmonic-plus-noise (HPN) baseline with the same encoder/decoder:
  - harmonic loss 0.088 vs 0.111 (21% better)
  - multi-resolution STFT loss 1.807 vs 1.899
  - total loss 0.949 vs 1.006 (5.7% better)
  
  No formal listening test and no real-time factor or CPU cost are reported. — [arXiv 2603.09391 HTML](https://arxiv.org/html/2603.09391v1)
- Limitations stated by the authors: validation on real recordings is still needed, resonator timbre matching needs work, and turbo and drivetrain sounds are future work. Code, weights and audio are released under CC BY-NC-SA 4.0 (non-commercial). — [arXiv 2603.09391 HTML](https://arxiv.org/html/2603.09391v1); [project page](https://rdoerfler.github.io/ptr-model-page/)
- Origin: Robin Doerfler's UPF master's thesis (Aug 2025, CC-BY-4.0), which introduces the "Procedural Engines Model" with HPN and PTR variants and harmonic-focused losses inspired by NVH analysis. It notes that fundamentals can be as low as 5 Hz. — [Zenodo 17304643](https://zenodo.org/records/17304643)

**Doerfler, Kuntz, Zimmer — EONE, arXiv 2606.21521 (2026)**
- Representation: 64 engine orders (integer and half-integer multiples of the firing frequency) plus 64 ERB-spaced filtered noise bands. Gains are learnable curves over rpm and torque, with 40 points each (250 rpm and 25 Nm resolution). — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- Training has two stages:
  1. Pretrain a timbre encoder/decoder on a diverse corpus: semi-anechoic dyno recordings plus field recordings, several vehicles, several exhaust configurations and mic positions, plus the Procedural Engine Sounds dataset.
  2. Fine-tune only the gain curves on the target vehicle's recordings, with the encoder frozen.
  
  Result: it needs recordings of the target engine with time-aligned rpm and torque. — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- Runtime: no neural network runs at inference. Harmonics come from interpolated lookup-table oscillators and noise bands are pre-rendered via inverse real FFT, so it deploys on embedded targets inside the "EVx Suite" automotive sound platform. — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- Listening test: 12 listeners (6 expert, 6 naive) rated realism on a 0–100 scale. 27 stimuli × 3 conditions × 3 repetitions = 243 trials.

  | Condition | Naive listeners | Expert listeners |
  |---|---|---|
  | Recording (REC) | 61 | 76 |
  | EONE | 61 | 71 |
  | EOE baseline (36 harmonics, no noise; "representative of current automotive sound design") | 35 | 35 |

  EONE vs REC: not significant for naive listeners (p=0.13), significant for experts (p<0.03). Mean log-spectral distance is 4.9 dB. — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- Stated limitation: the model is only a function of rpm and torque, so it cannot capture "non-deterministic or occasional sound events, such as backfiring or turbo noises". Rarely observed operating points also have larger errors. — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)

**Procedural Engine Sounds dataset (Doerfler, arXiv 2603.07584)**
- Size and content: 19.0 h, 5,935 files, 24.5 GB, 48 kHz. V8, I6 and I4 petrol engines. rpm and torque are embedded as sample-accurate extra audio channels.
- How it was made: engine-order magnitudes and frequency deviations were analysed from 5–10 min of real recordings per vehicle, then resynthesized with a 128-oscillator harmonic-plus-noise synth plus pink-noise modulation and Karplus-Strong exhaust resonators. That is a 15–30× augmentation.
- License: CC BY-NC-SA 4.0.

  — [arXiv 2603.07584](https://arxiv.org/html/2603.07584v2); data at [Zenodo](https://doi.org/10.5281/zenodo.16883336) and [Hugging Face](https://huggingface.co/datasets/rdoerfler/procedural-engine-sounds)

**Motor2Synth — Lobato, Hank, Zhang, Luo (HEAD acoustics), AES AI/ML for Audio conf., London, Sept 2025**
- Standard DDSP (harmonic generator, noise filter, reverb) driven by f0 and loudness.
- Music pitch trackers such as CREPE failed on engine audio, so they built a self-supervised rpm estimator based on PESTO. That lets it learn from ordinary vehicle recordings with no CAN data.
- Purpose: resynthesize an existing engine as a base for EV Active Sound Design. It was demonstrated with a DDSP model of a high-performance V8 used in a sound variant for the Xiaomi SU7 / SU7 Ultra.
- The abstract gives no numeric listening-test results.

  — [HEAD acoustics abstract PDF](https://cdn.head-acoustics.com/fileadmin/data/global/Abstracts/Abstract-AES-2025-Motor2Synth-Leveraging-DDSP-for-Generating-ASD-Compatible-Combustion-Engine-Sounds.pdf)

**Li, Wang, Li — "Real-time Automotive Engine Sound Simulation with Deep Neural Network" (Z-one Tech / Duke Kunshan, NCMMSC 2023)**
- Method: a hybrid of two parts. (1) The idle recording (under 1 s) is pitch-shifted by rpm/idle-rpm and made click-free with a frame-level Griffin-Lim overlap-add method they call GLOLA. (2) Three small DNNs, each with 2 fully connected layers of 128 units, predict the amplitude of F0, 2F0 and 3F0. Each takes 44 inputs: 11 frames of RPM, pedal pressure (POP) and their deltas. — [paper PDF](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf)
- Data: 4 indoor recordings of an MG3 sedan (4-stroke I4), about 55 min in total (3327 s), rpm range 690–6284. rpm and POP were logged every 10 ms, and audio was downsampled to 4 kHz. GLOLA runs 40 iterations per 10 ms frame. The evaluation is only qualitative (spectrogram and amplitude plots): no MOS and no CPU numbers. — [paper PDF](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf)

**DDSP conditioned on CAN-bus signals (AES e-library id 23427; authors and year not retrieved, page returned 403)**
- Compares crank-based and firing-based f0. Inputs are combinations of rpm, gear, accelerator pedal, vehicle speed and longitudinal acceleration, fed either directly or through an encoder.
- Findings: crank-based f0 was more accurate for a 4-cylinder 4-stroke. More complementary signals help. Encoded conditioning is best on objective metrics when few signals are available, while direct conditioning gives the best perceptual results with the full signal set. — [AES e-library 23427](https://aes.org/publications/elibrary-page/?id=23427) (details via search snippet only)

**GAN and diffusion**
- No dedicated engine-sound GAN or diffusion paper from 2023–2026 turned up. The nearest is automotive time-series and audio augmentation (Engler et al., ICANN 2025) plus general Foley diffusion work (DCASE 2023/2024 Foley and scene synthesis). — [Springer ICANN 2025](https://link.springer.com/chapter/10.1007/978-3-032-04549-2_27); [DCASE 2023 Foley](https://arxiv.org/pdf/2304.12521)

### Inferences
- **Trend:** the field is moving away from black-box neural generation toward "differentiable parametric synth + small controller network". EONE removes the network at runtime entirely. For real time that is cheap: tens of oscillators and noise bands, or a Karplus-Strong bank.
- **Generalization gap:** all of these are identification methods. They reproduce an engine that was recorded. For an arbitrary designed engine, the practical role of such models is:
  1. a fitted residual or timbre layer added on top of physics (see Q3), or
  2. a prior pretrained on many engines whose parameters are then predicted from the physics model's output rather than from recordings. This is untested in the literature.
- **Benchmark:** the EONE listening numbers are the best available bar to beat. A purely harmonic order synth scores about 35/100, harmonics plus a learned broadband-noise layer scores about 61–71, and recordings score 61–76. So the stochastic, load-dependent noise layer is a major contributor to perceived realism.
- **Licensing:** PTR code and data are CC BY-NC-SA, so they cannot ship in a commercial product without a separate license.

### Gaps
- None of the neural models has been evaluated on engines absent from training. PTR's claim that one model covers I4 and V8 applies only to its training configurations.
- No real-time factor, CPU or latency figures for PTR. The EONE paper says "embedded-compatible" but gives no CPU numbers.
- I could not retrieve authors and year of the AES CAN-bus DDSP paper (403). Motor2Synth listening results are unknown (abstract only).
- I found no peer-reviewed engine-specific GAN or diffusion work with listening tests.

## Q2 — Differentiable physical models: learning physics or waveguide engine parameters from recordings

### Takeaway
It is feasible and already partly demonstrated: PTR puts differentiable Karplus-Strong waveguides (exhaust resonators) inside a gradient-trained engine model. The general toolbox is mature: differentiable IIR, all-pole and all-pass filters, differentiable modal synthesis, and perceptual sound-matching losses. Nobody has yet published gradient fitting of a full 1-D gas-dynamics engine model to recordings. The known risks are recursive-filter instability and poorly conditioned losses for physical parameters.

### Cited Findings
- **PTR:** it routes firing-locked pulse trains through recursive, differentiable Karplus-Strong resonators that simulate exhaust acoustics. It is trained end-to-end with a multi-resolution STFT loss (FFT 32–65,536) plus an engine-order harmonic loss (FFT 65,536, window 16,384). — [arXiv 2603.09391 HTML](https://arxiv.org/html/2603.09391v1)
- **Differentiable filters:**
  - Differentiable IIR filters for ML: Kuznetsov, Parker and Esqueda, DAFx 2020 (per the DDSP review).
  - Differentiable all-pass cascades fitted to black-box phase responses (DAFx 2023).
  - Differentiable time-varying all-pole filters (2024).
  
  — [DDSP review, Hayes et al. 2023](https://arxiv.org/pdf/2308.15422); [Differentiable all-pass filters, DAFx23](https://arxiv.org/pdf/2306.00860); [Differentiable all-pole filters](https://arxiv.org/pdf/2404.07970)
- **Known issues:** recursive (IIR) filters and phase accumulation can be numerically unstable during training. The usual fixes are constrained parameterizations or truncated backpropagation through time (TBPTT). — [DDSP review (Frontiers 2023)](https://www.frontiersin.org/journals/signal-processing/articles/10.3389/frsip.2023.1284100/full)
- **Differentiable physical sound matching in other domains:**
  - DiffSound: differentiable modal sound rendering that infers shape, material and impact position from audio.
  - Perceptual-Neural-Physical sound matching.
  - "Learning to solve inverse problems for perceptual sound matching".
  - A 2026 paper evaluating loss functions for out-of-domain differentiable sound matching.
  
  — [DiffSound](https://arxiv.org/pdf/2409.13486); [PNP sound matching](https://arxiv.org/pdf/2301.02886); [arXiv 2311.14213](https://arxiv.org/pdf/2311.14213); [arXiv 2608.27698](https://arxiv.org/pdf/2608.27698)
- **Prior art for waveguide engine models:** Baldan et al.'s physically informed car engine synthesizer (GeneCars) uses a digital waveguide for the exhaust. — [ResearchGate, Baldan et al.](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments)

### Inferences
- **Most practical near-term use for a physics engine:** keep the gas-dynamics core fixed and learn only a small set of "calibration" parameters from one or a few recordings of any engine, by differentiating through a differentiable version of the linear post-chain. Candidates:
  - tailpipe radiation filter
  - muffler waveguide lengths and losses
  - mechanical-noise band gains
  - cabin EQ

  The fitted values then act as generic, physically meaningful presets that carry over to user-designed engines.
- **Harder:** differentiating a full nonlinear 1-D gas-dynamics solver (shocks, valve flow) is possible in principle with autodiff but would be slow and badly conditioned. That is an inference; I found no source attempting it.

### Gaps
- I found no published work on gradient-based identification of a full engine gas-dynamics or waveguide model (intake plus cylinders plus exhaust) from recordings.
- The survey "Four Decades of Digital Waveguides" (arXiv 2604.12878) appeared in results but was not read.

## Q3 — Hybrid: physics for pulses and gas dynamics + learned or measured noise, residual or IRs

### Takeaway
Hybrids are the norm in both research and industry: deterministic, harmonic or pulse structure comes from a model, and broadband, stochastic and path effects come from learned or measured data. Examples:
- a physics-informed pulse or order model plus learned noise bands (PTR, EONE)
- sample pitch-shifting plus DNN harmonic correction (Li et al.)
- a physical pressure model convolved with measured or designed IRs (engine-sim)
- measured or CAE source strengths times measured transfer functions (B&K NVH Simulator, Pieren et al.)

### Cited Findings
- **EONE:** 64 orders plus 64 learned ERB noise bands. Adding the noise layer raised realism from 35 (36 harmonics only) to 61–71 out of 100. — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- **PTR:** load-gated noise (throttle gate and DFCO gate) on top of physics-structured pulse trains. — [arXiv 2603.09391 HTML](https://arxiv.org/html/2603.09391v1)
- **Li/Wang/Li:** a sample-based idle loop plus a DNN correction of the F0–3F0 amplitudes. — [paper PDF](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf)
- **engine-sim (AngeTheGreat, MIT license):** simulated pressure signals are convolved with impulse responses. Users can load .wav IRs via `impulse_responses.mr`, and convolution level is a user control. Development moved to a "community edition" repo. — [engine-sim GitHub](https://github.com/ange-yaghi/engine-sim); [Community edition releases](https://github.com/Engine-Simulator/engine-sim-community-edition/releases); [Starlog write-up](https://starlog.is/articles/developer-tools/ange-yaghi-engine-sim/)
  - Caveat: the Starlog article says the IRs model "exhaust, intake resonance, and even microphone placement". That is a secondary source.
- **EngineLab (third-party, C++20/JUCE):** synthesizes the exhaust from simulated cylinder pressure through a physical duct network. It says its exhaust IRs in `assets/ir/` come from Engine Sim 2D under MIT, with per-file provenance in `assets/ir/README.md`. — [EngineLab GitHub](https://github.com/zolaski333/EngineLab) (via search snippet)
- **B&K NVH Simulator:** mixes measured vehicle NVH with CAE data. Engineers can swap in individual source strengths and transfer functions (for example a single engine mount) and toggle or filter airborne and structure-borne contributions in time-domain source-path-contribution (SPC). — [B&K NVH simulation](https://www.bksv.com/en/knowledge/applications/nvh-noise-vibration-harshness/nvh-simulation); [B&K NVH Simulator](https://www.bksv.com/en/analysis-software/nvh-software/vehicle-nvh-simulator)
- **Pieren et al. (Empa, Applied Sciences 2016):**
  - Pass-by auralizer. Propulsion sound comes from spectral modeling synthesis as a function of engine speed, load and emission angle, with parameters measured on a chassis dynamometer.
  - Propagation uses time-variant filters for delay, Doppler, spherical spreading, ground reflection and air absorption.
  
  — [Pieren et al. (ResearchGate)](https://www.researchgate.net/publication/288057515_Auralization_of_Accelerating_Passenger_Cars_Using_Spectral_Modeling_Synthesis); [Empa TAURA](https://www.empa.ch/web/s509/taura)

### Inferences
- **Recommended hybrid for arbitrary designed engines:**
  1. Physics produces the deterministic pressure at the valves and tailpipe (orders, inter-cylinder irregularity, pulse shape).
  2. Add a parametric, load-gated broadband layer (combustion roughness, flow noise) whose spectral shape is set by generic priors learned from multiple recorded engines (EONE-style ERB bands), scaled by physics quantities such as mass flow, pressure-rise rate and gas velocity.
  3. Pass everything through a fixed path layer (radiation, structure, cabin or exterior propagation, room) built from measured or synthesized IRs.

  This keeps the generality of physics while adding the stochastic content that the listening tests show matters.

### Gaps
- No published hybrid evaluates listening realism specifically for physics-generated, not recording-fitted, engine sounds.

## Q4 — Realism layers needed on top of a gas-dynamics model

### Takeaway
A gas model gives exhaust and intake orifice sound. Real recordings also contain the following layers:
1. **Mechanical, structure-borne sources:** piston slap, valve train, gears and chain, injector and high-pressure-pump ticks.
2. **Block and cover radiation.**
3. **Tailpipe radiation:** nearly omnidirectional at the dominant low frequencies.
4. **Path to the listener:**
   - cabin: structure-borne booming below about 400 Hz, a first longitudinal cabin mode around 40–75 Hz, airborne sound insulation acting as a low-pass
   - exterior: distance, ground reflection, Doppler, air absorption
   - room or outdoor reverb
5. **Cyclic variability.**

Free IR resources exist: engine-sim IRs (MIT, provenance unclear) and OpenAIR rooms and outdoor spaces (per-file Creative Commons). I found no free measured vehicle cabin or exhaust transfer-function library.

### Cited Findings

**Mechanical and structure-borne sources**
- **Diesel noise-source map** (Cummins/MDEC 2015 slides): combustion, injector, valve train, gear train, piston slap, crankshaft dynamics, turbo, fan and exhaust.
  - Structure-borne paths radiate mainly via the oil pan and covers. Combustion forces on piston and rod correspond to "~170 dB re 20 µPa" cylinder pressure.
  - Heavy-duty diesel levels at rated speed and load were about 94–108 dB(A) average SPL at 1 m (1970–2000 trend), rising as injection and cylinder pressures grew.
  - Sound quality depends on pure tones, high-frequency level, impulsiveness and cycle-to-cycle combustion variation, not only on overall level.
  
  — [MDEC 2015, Stirling, "Diesel Engine Noise"](https://mdec.ca/2015/S6P4_stirling.pdf)
- **Piston slap:** lateral rod-force components drive the piston across its clearance into the liner. The impacts excite structure-borne noise and are "the main cause of the predominant high frequency noise" in diesels. — [Fielding & Skorecki 1969 (SAGE)](https://journals.sagepub.com/doi/10.1243/PIME_PROC_1969_184_063_02); [Piston-slap force reconstruction, PMC 2024](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC11207819/) (via search summary)
- **GDI fuel system ticks:**
  - System noise spans roughly 1.6–16 kHz. High-pressure pumping sits around 1.6–5 kHz, and the digital inlet valve "ticking" around 5–10 kHz.
  - Up to three ticks per pump event: inlet valve hitting its stop, closing against the plate, and bounce.
  - Most audible at idle, when the rest of the engine is quiet.
  
  — search summary drawn from patent literature ([US 11293390](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/11293390); [US 8245693 idle tick reduction](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8245693)). Treat the band edges as indicative: I did not read the patent text directly.

**Exhaust tailpipe radiation**
- Alfredson & Davies (J. Sound Vib. 1970), measurements on a running engine:
  - In-pipe pressures peak at about 155 dB at 300 Hz and about 135 dB at 1 kHz.
  - Total radiated level is about 120 dB at 3 ft (0.9 m) from the outlet.
  - Radiated sound showed a "lack of directivity" and 1/r decay, i.e. spherically diverging and monopole-like, at the dominant low frequencies.
  - Jet (flow) noise was insignificant in their case.
  - Mean flow raises the outlet reflection coefficient but barely changes its phase.
  
  — [Alfredson & Davies 1970 (PDF)](https://internoise2018.org/assets/documents/1_Alfredson_and_Davies.pdf)
- Pass-by modelling matches well for a departing vehicle, but deviates as it approaches the microphone when the body shields the tailpipe. The cause is attributed to insufficient source-directivity fidelity. — search summary of [ISO 362 pass-by literature review](https://www.researchgate.net/publication/256658573_Noise_source_characteristics_in_the_ISO_362_vehicle_pass-by_noise_test_Literature_review)
- Exhaust spectra are dominated by firing-frequency multiples up to about 1 kHz. — search summary citing [ScienceDirect "Flow noise" topic page](https://www.sciencedirect.com/topics/physics-and-astronomy/flow-noise)

**Cabin transfer**
- Two paths: structure-borne (mounts into the body) and airborne (engine bay into the cabin). Below about 400 Hz, the difference between total interior SPL and airborne SPL is attributed to structure-borne noise, and much of the annoyance sits below about 600 Hz. — [INTERNOISE 2014 p110](https://acoustics.asn.au/conference_proceedings/INTERNOISE2014/papers/p110.pdf); [TPA technical note](https://www.researchgate.net/publication/288702928_Technical_note_Vehicle_interior_noise_source_contribution_and_transfer_path_analysis) (via search summary)
- **Booming:** the first longitudinal cabin mode (1,0,0) is typically about 65–75 Hz. One estimate of 69 Hz equals I4 firing frequency at about 2070 rpm, and European I4 cars show interior booms at 2000–2500 rpm. Some cars show a 40 Hz boom from the first longitudinal mode.
  - Order 2 of an I4 at 1200–6000 rpm sweeps 40–200 Hz, which is the main booming band.
  
  — [ANU, "Recent advances in ANC inside automobile cabins"](https://openresearch-repository.anu.edu.au/server/api/core/bitstreams/7a492a53-4347-4133-9017-ea07464f8a7a/content); [Booming investigation, LCV](https://www.researchgate.net/publication/318277231_An_Investigation_of_Booming_Noise_on_a_Light_Commercial_Vehicle) (via search summary)
- Rumbling noise, driven by engine combustion forces, is identified with in-situ blocked-force transfer path analysis (TPA). — [Applied Acoustics, BF-TPA rumbling](https://www.sciencedirect.com/science/article/abs/pii/S0003682X18308569)

**Exterior microphone and environment**
- Pieren et al. used time-variant filters for propagation delay, Doppler, geometrical spreading, ground reflection and air absorption, plus emission-angle-dependent source spectra measured on a dyno. — [Pieren et al. 2016](https://www.researchgate.net/publication/288057515_Auralization_of_Accelerating_Passenger_Cars_Using_Spectral_Modeling_Synthesis)
- TPA and synthesis in auralization: the listener signal is built from source signals times transfer functions, with binaural filters, "to achieve correct level and colouration". — [Springer, Transfer Path Analysis and Synthesis](https://link.springer.com/chapter/10.1007/978-3-030-51202-6_15)

**Free IR availability and licensing**
- **engine-sim:** the repo is MIT licensed, ships IR .wav files and loads user IRs via `impulse_responses.mr`. — [engine-sim GitHub](https://github.com/ange-yaghi/engine-sim); [Engine sound utility](https://beejmaxx.github.io/engine-sim/)
  - There is also a community "Better Impulse Response Library" on the engine-sim parts catalog. Its license was not verified. — [catalog.engine-sim.parts](https://catalog.engine-sim.parts/parts/1563)
- **OpenAIR (University of York):** a room and outdoor IR library. Most content is under Creative Commons, and the contributor chooses the specific CC license per item (or reserves all rights), so licenses must be checked file by file. — [OpenAIR About](https://www.openair.hosted.york.ac.uk/?page_id=2); [York project page](https://www.york.ac.uk/physics-engineering-technology/research/communication-technologies/projects/open-acoustic-impulse-response-library/)
- **Commercial outdoor IRs** exist, for example BOOM Library "Outdoor Impulse Responses" (paid). — [BOOM Library](https://www.boomlibrary.com/sound-effects/outdoor-impulse-responses/)

### Inferences
- **Minimum layer stack after the gas model**, ordered by likely perceptual payoff (my inference, based on the EONE noise-layer result and the NVH literature):
  1. Load-gated broadband combustion and flow noise with cycle-to-cycle variation.
  2. Tailpipe and intake radiation filter: roughly an omnidirectional monopole below about 1 kHz. At higher frequencies an unflanged-pipe high-pass and directivity roll-off apply, following standard Levine–Schwinger behaviour (textbook, not sourced here).
  3. Exterior propagation with distance, ground reflection (comb filtering from a single image source) and air absorption, or a cabin chain made of:
     - a structure-borne low-frequency path with a boom resonance around 40–75 Hz
     - airborne transmission-loss low-pass
     - cabin reverb
  4. Mechanical impulse layers triggered from crank angle:
     - piston slap near TDC and BDC (high-frequency, structure-filtered)
     - valve seating clicks at cam events
     - GDI ticks (5–10 kHz) per injection or pump event
     - gear or chain whine at tooth-mesh orders
  5. Room or outdoor IR convolution.
- **What works without recordings:** layers 2, 3 and 5 can be generic and parametric for any designed engine. The broadband spectra (layer 1) and mechanical levels (layer 4) are where measured priors help most.
- **Calibration target:** levels relative to the exhaust matter more than absolute values. The MDEC data implies diesel mechanical and combustion noise reaches about 100 dB(A) at 1 m on heavy engines. For petrol cars in the cabin, exhaust and intake orders dominate under load, while ticks dominate at idle.

### Gaps
- I found no free, openly licensed measured datasets of vehicle cabin transfer functions, engine-block radiation FRFs or tailpipe directivity. The MDPI Eng 2025 paper "A Method for Analysing In-Vehicle Acoustic Response to Engine Excitation" (doi 10.3390/eng6110285) returned 403.
- I found no quantified per-source levels (dB) for petrol valve train, timing chain or piston slap in 2020–2026 sources. Only qualitative descriptions and frequency ranges were found.
- The provenance and recording method of engine-sim's IRs are undocumented in the sources I read. Whether they are measured or synthesized is unknown.

## Q5 — What commercial and pro tools do, and where their realism comes from

### Takeaway
Pro tools get realism from measurement, not first-principles physics:
- **Order-based and noise synthesis tuned on recorded order analyses:** HEAD acoustics ArtemiS/SQuadriga, Siemens Simcenter ASD.
- **Measured source strengths times measured or CAE transfer functions:** B&K NVH Simulator, AVL.
- **Granular playback of recorded rpm ramps:** Crankcase REV in games.

The latest step is learned order-plus-noise models fitted to recordings (EONE in EVx Suite, Motor2Synth at HEAD). None of them synthesizes an engine that was never built.

### Cited Findings
- **Siemens Simcenter Active Sound Design:** "several sound design methods and real-time synthesis based on various engine parameters, such as speed, throttle position and torque". It is used for design, validation, tuning and deployment of interior sound enhancement and exterior AVAS sounds. A Hyundai case study exists. — [Simcenter ASD solution brief](https://resources.sw.siemens.com/en-US/solution-brief-simcenter-active-sound-design-for-automotive/); [Simcenter blog: ASD for mass production](https://blogs.sw.siemens.com/simcenter/active-sound-design-deploy-mass-production/); [Hyundai case study](https://resources.sw.siemens.com/en-US/case-study-hyundai/)
- **HEAD acoustics:**
  - SQuadriga III is a mobile 8-channel recorder with optional order-analysis firmware.
  - The ArtemiS SUITE Sound Engineering Project lets users selectively remove or synthesize orders and components to build target sounds.
  - HEAD's own research (Motor2Synth) uses DDSP fitted to recordings.
  
  — [SQuadriga III](https://www.head-acoustics.com/products/data-acquisition/squadriga-iii); [ArtemiS SUITE modules](https://www.head-acoustics.com/products/analysis-software/artemis-suite-modules); [Advanced Filters / Sound Engineering data sheet](https://global.head-acoustics.com/downloads/eng/artemis/D5019_ArtemiS_suite_Advanced_Filters_Module_e.pdf)
- **Industry-baseline realism:** EONE describes parametric engine-order frameworks as "among the most practical foundations" for deployable ASD but limited in realism. Its EOE baseline (36 orders, no noise), representing current automotive practice, scored 35 out of 100. — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- **Brüel & Kjær (HBK):** the NVH Simulator and CAE Sound & Vibration Auditioner build interactive vehicle NVH from a mix of measured and CAE data. They use time-domain source-path-contribution with per-path source strengths and transfer functions (airborne and structure-borne) that can be toggled and filtered. — [B&K NVH Simulator](https://www.bksv.com/en/analysis-software/nvh-software/vehicle-nvh-simulator); [CAE Auditioner](https://bksv.com/en/products/Analysis-software/vehicle-noise-vibration-and-harshness-software/cae-auditioner-8601-x)
- **AVL:** NVH testbeds from component to full vehicle, plus acoustic performance prediction for interior and exterior sound design. No public technical detail on its synthesis method was found. — [AVL acoustic performance prediction](https://www.avl.com/en/testing-solutions/e-mobility-testing/nvh-testing/acoustic-performance-prediction)
- **Games — Crankcase REV** (Wwise, FMOD and Unreal integration):
  - A granular plus auto-looping hybrid working in the frequency domain.
  - It scrubs forward or backward through recorded acceleration and deceleration ramps ("RAMPs": a smooth idle to max rpm and back) at any rate, including steady state.
  - Realism comes entirely from the recordings.
  
  — [Audiokinetic: Crankcase REV](https://www.audiokinetic.com/products/plug-ins/crankcase-rev/); [Audiokinetic blog: sampling to granular](https://www.audiokinetic.com/en/community/blog/engine-sound-modeling-from-sampling-to-granular-synthesis-in-wwise/); [Designing Sound: Project CARS / Forza 5 / REV](https://designingsound.org/2014/08/11/vehicle-engine-design-project-cars-forza-motorsport-5-and-rev/)

### Inferences
- **Industry practice:** measure the real car (orders, noise, transfer paths), then parametrize. Physics-based generation is confined to CAE (FE/BE, 1-D gas-dynamics tools) feeding the auralizer with source strengths, and even there the transfer functions are usually measured.
- **Differentiator for a physics engine:** it is the only route to a plausible sound for a new or user-designed engine. Its realism ceiling is set by the non-physics layers (Q4), which pro tools take from measurements.

### Gaps
- I could not access the Siemens Sound Designer technical page (JS-only). The exact synthesis internals (granular vs order vs hybrid) and HEAD's ASD runtime internals are not publicly documented in the sources retrieved.
- No public listening-test numbers comparing commercial ASD engines to recordings, other than EONE's EOE-baseline proxy.
- AVL's specific engine sound synthesis tooling was not documented publicly in sources found.
