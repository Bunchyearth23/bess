# State of the art: procedural, physically based and hybrid synthesis of IC-engine sound (games, simulators, automotive ASD)

Research date: 2026-09-28. Scope: techniques that run in real time, what realism each achieves, how each fails, and implementation details that apply to a Rust per-firing-pulse + noise + 1D-waveguide synth whose idle and deceleration sound artificial.

Source-access note: several primary PDFs returned HTTP 403 to automated fetch: the Baldan 2015 paper (IUAV, ResearchGate, academia.edu), the Jagla 2012 full text (HAL), and Dupré et al. 2023 (Acta Acustica). For Baldan, I went to the **open-source reference implementation** (the Sound Design Toolkit `SDTMotor.c`, from the same research group), which is a better primary source for implementation details. For engine-sim I read the **source code** directly (`synthesizer.cpp`, `jitter_filter.h`, `piston_engine_simulator.cpp`). Code-level numbers below come from those files.

---

## 1. Academic methods: what each does, what realism it reaches, how it fails

### Takeaway
The literature splits into (a) sample/analysis-resynthesis methods such as PSOLA, wavetable, phase vocoder and order-tracked additive synthesis, which sound most realistic but only near the recorded data, and (b) procedural/physical methods such as Farnell's warped waveguides, Baldan/SDT's valve-modulated waveguide network, and gas-dynamics simulators like engine-sim and EngineLab, which are flexible but sound "synthetic" unless they include three things: stochastic cycle-to-cycle variation that is **coupled to the pulses** (not added beside them), realistic resonant transfer paths, and distinct physics per operating mode (combustion vs. motoring/fuel cut-off). The newest work (2025–2026) is physics-informed DDSP. It argues that modelling the **pulse train** (the cause) beats modelling the harmonic spectrum (the result).

### Cited Findings

**Baldan, Lachambre, Delle Monache, Boussard (2015), "Physically informed car engine sound synthesis for virtual and augmented environments"**
- Presented at the IEEE 2nd VR Workshop on Sonic Interactions for Virtual Environments (SIVE) 2015. It is a procedural, physically informed model offered "as an effective, flexible and computationally efficient alternative to sample-based and analysis/resynthesis approaches". It was built as a Max/MSP external and integrated into GeneCars (a driving simulator for industrial sound design) and SkAT Studio — [IEEE Xplore](https://ieeexplore.ieee.org/abstract/document/7361287); [ResearchGate](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments)
- Its implementation is `SDTMotor.c` in the open Sound Design Toolkit. The structure, from the code:
  - **Per-cylinder four-stroke cycle.** For phase φ∈[0,1), `pressure = cos(4πφ)`. The intake valve is `sin(4πφ)` gated to 0.5<φ<0.75. The exhaust valve is `−sin(4πφ)` gated to 0.25<φ<0.5 — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Spark/combustion.** A single sine half-burst: `spark = sin(2π·φ/sparkTime)·(φ<sparkTime)·throttle`, with default `sparkTime = 0.1` of the cycle. **Combustion amplitude scales linearly with throttle, so at throttle 0 only the compression "pressure" term and the valve-modulated resonances remain.** This is how the model represents coasting/motoring — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **The cylinder is itself a waveguide** whose delay is modulated by piston position: `delay = cylinderSize · (1 − (0.5·pressure+0.5)·(1 − 1/compressionRatio))`, with defaults cylinderSize 500 and compressionRatio 10 — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Valves are time-varying reflection coefficients** between the waveguides: `feed = valve·JOINT_FEED(0.1) + (1−valve)·METAL_FEED(0.9)`. A closed valve reflects 0.9 and an open one 0.1 — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Intake noise** is `inValve · onePoleLowpass(whiteNoise)` injected into a per-cylinder intake waveguide (open-end reverse feedback −0.5). In other words, the noise is **gated by the valve-opening envelope, not continuous** — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Exhaust network:** per-cylinder "extractor" (header) waveguides → one "exhaust" waveguide → **4 parallel muffler waveguides** with delays `mufflerSize·(0.7 + 0.6·i/3)` (0.7×, 0.9×, 1.1×, 1.3×) → an "outlet" waveguide with open-end feedback −0.5 — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Header lengths are deliberately unequal per cylinder:** `extractorSize·(1 ± k/12)` alternating sign. The intake runners get the same treatment. **Firing is also made unequal** via `asymmetry`: phase offset `0.5·asymmetry·sin(2π·position)/nCyl`, default 0.1 — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Backfire (decel pops):** a flag `isRevvingDown` is set when rpm decreases. At each cycle wrap, a pop fires with probability `backfireRate·isRevvingDown`. After each pop `backfireRate *= backfire`, so pop probability decays geometrically during an overrun. It resets to `backfire` when rpm rises again. The pop is a sine burst injected into the mufflers — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **Three separate outputs:** intake, engine-block "vibrations" (sum of pressure+valves+spark through a one-pole lowpass), and exhaust outlet. Each is DC-blocked (default `dc = 20` Hz, `damp = 20`) — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - **No per-cycle random amplitude or timing variation is applied** to the combustion pulses. The only stochastic parts are the intake noise and the backfire trigger — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
- A JS/WebAudio/WASM port by Antonio-R1 uses these example sizes: intake/exhaust/extractor waveguides 100 samples, straight pipe 128 samples, muffler elements [10, 15, 20, 25] samples, outlet 5 samples, open-end reflection 0.01, closed 0.95 — [engine-sound-generator](https://github.com/Antonio-R1/engine-sound-generator)
- I found no formal listening-test results published for the Baldan model. Searches only show a figure caption about "user testing the sound in the listening environment" in GeneCars — [ResearchGate figure](https://www.researchgate.net/figure/Up-user-testing-the-sound-in-the-listening-environment-Down-GeneCars-GUI_fig7_280086598)
- Doerfler & Wyse (2026) group Baldan with procedural methods that "explicitly simulate combustion or mechanical processes but lack the adaptability and expressiveness of data-driven models" — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)

**Farnell, "Designing Sound" (MIT Press, 2010), Practical 22: Car Engines**
- The book's Pure Data engine patches include a "four-stroke engine with jitter": phasor → cosine waveshaping per cylinder, 4 cylinders with slight timing variations. It also has a "waveguide engine" with "warped waveguide" exhaust (delread~/delwrite~), overtone generators, and noise → bandpass sources — [ASPress Practical 22](https://aspress.co.uk/sd/practical22.html)
- Secondary description in the Baldan paper's figure: Farnell used digital waveguides "with anti-phase frequency-modulated delays" for the exhaust (the "warping circular waveguide"), driven by a ramp generator mimicking mechanical inertia. An "engine vibration function" is a parabolic waveshaper for low-frequency body resonance, and overtone generators inject high-frequency bursts at fixed cycle positions for mechanical transients — [ResearchGate figure (Farnell block diagram)](https://www.researchgate.net/figure/Block-diagram-of-Andy-Farnells-engine-sound-synthesis-model_fig2_280086598); [Designing Sound PDF](https://wp.ufpel.edu.br/labcomp/files/2022/09/PureData-Designing_Sound-Andy_Farnell.pdf)

**Jagla, Maillard, Martin (JASA 132(5):3098–3108, Nov 2012), enhanced PSOLA**
- Sound samples are extracted from a single recording with continuously varying engine speed (a run-up). Each sample holds "the sound emitted during one cycle of the engine plus the necessary overlap". Engine periodicity locates the extraction instants, which is the extension over speech PSOLA. At synthesis, samples matching the target rpm trajectory are concatenated by overlap-add. The authors claim "high quality audio restitution with a low computational load", suitable for real time — [PubMed abstract](https://pubmed.ncbi.nlm.nih.gov/23145595/); [JASA](https://pubs.aip.org/asa/jasa/article-abstract/132/5/3098/854169/Sample-based-engine-noise-synthesis-using-an); [HAL](https://hal.science/hal-00730614)
- Key design point: the grain is **one full engine cycle (720° for 4-stroke)**, not one firing interval. That preserves per-cylinder differences and half-order content — [Yumpu copy of paper](https://www.yumpu.com/en/document/view/5548034/sample-based-engine-noise-synthesis-using-an-enhanced-pitch-)

**Other analysis/resynthesis methods (from the related-work section of Li, Wang & Li, 2024)**
- Heitbrink & Cable (Driving Simulation Conf. NA 2007): wavetable with crossfading between samples. Amman & Das (IEEE TIE 2001): deterministic-stochastic decomposition, with the deterministic part from the synchronous DFT and the stochastic part from multi-pulse-excited time-series modelling. Janse van Rensburg et al. (2006): phase vocoder. Min, Park & Park (Shock & Vibration 2018): summed crank harmonics plus random combustion noise with a measured-like spectrum. Lee et al. (AES 154, 2023): wavetable. Lazaro et al. (JAES 70(4) 2022): EV sound via granular synthesis — [Li, Wang, Li 2024 PDF](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf)

**Neural / differentiable approaches (2023–2026)**
- **Li, Wang, Li (NCMMSC 2023 / Springer 2024).** A hybrid method: an idle recording is pitch-shifted by rpm/idle_rpm at frame level. Three small MLPs (2×128 FC, input = 11 frames of RPM, pedal and their deltas, 44 dims) predict amplitude corrections for F0, 2F0 and 3F0. The result is resynthesized with a modified Griffin-Lim with overlap-add ("GLOLA") to remove inter-frame clicks. Cost is about 8 ms per 10 ms frame on one 2.2 GHz Xeon core (pitch shift 0.4 ms, DNN 0.8 ms, GLOLA 6–7 ms for 40 iterations). Evaluation is spectrogram/order-amplitude comparison only, **no listening test** — [PDF](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf); [Springer](https://link.springer.com/chapter/10.1007/978-981-97-0601-3_15)
- **Motor2Synth (Lobato, Hank, Zhang, Luo; AES AIMLA conf., London 2025).** DDSP harmonic+noise+reverb conditioned on f0/loudness, aimed at ASD. They note that real recordings' "chaotic nature and significant RPM fluctuations make them challenging to process for sample-based synthesis" — [search summary / citing arXiv](https://arxiv.org/html/2603.09391v1)
- **Doerfler & Wyse (arXiv 2603.09391, 10 Mar 2026), "Physics-Informed Neural Engine Sound Modeling with Differentiable Pulse-Train Synthesis" (PTR)** — [arXiv](https://arxiv.org/html/2603.09391v1)
  - One periodic impulse train per cylinder, phase-offset by the real firing order (V8 1-5-4-8-6-3-7-2), with **learned per-cylinder timing offsets of up to ±40° crank angle**.
  - The pulse is the derivative of a sum of cosines (a bipolar pressure-gradient pulse, not a step). Harmonic rolloff is `a_k ∝ exp(−0.5·k·λ(t))`.
  - The pressure-release envelope is `E(φ) = (1−exp(−αφ))·exp(−βφ)`, with learned time-varying attack α and decay β.
  - "Thermodynamic phase modulation" `φ_mod = 2π(φ/2π)^ν(t)` compresses the pulse onset and stretches the tail. This models the faster propagation of the hot leading edge.
  - There are three stochastic layers:
    - turbulence as stochastic AM during throttle;
    - intake pulsations with an exponential envelope locked to engine phase;
    - **steady airflow noise for deceleration fuel cut-off (DFCO)**, as an ERB-spaced noise bank with learned gains.
  - Exhaust resonance uses an extended Karplus-Strong comb `y[n] = x[n] + g·(α·y[n−L] + β·y[n−L−1])`.
  - Conditioning is rpm, ΔRPM, Δ²RPM, torque and its deltas, a throttle factor `max(torque, ε)^0.7`, and a DFCO factor `max(−torque, ε)`, with ε = 0.02 keeping a minimum gain at idle. The model runs at 125 Hz frames and 16 kHz audio.
  - Result versus a DDSP harmonic-plus-noise baseline: harmonic loss 0.111 → 0.088 (−21%), total loss 1.006 → 0.949. **No listening test.** Real-time inference speed is not reported.
  - Their stated reason for the gain: the harmonic+noise baseline models "the observed harmonic spectrum—rather than the physical cause: the sequential pulse structure". Phase coherence of firing-pattern-coupled pulses is the stronger bias. They report the pulse model reproduces intermittent combustion at clutch disengagement and "clear articulation of individual combustion events at low RPM transitioning to dense harmonic textures at high RPM".
- **Doerfler (arXiv 2603.07584, 2026), procedural dataset generator.** 128 oscillators at engine orders 0.5…64, with frequencies `f_h = (h + δ_h(t))·f0` using **measured order deviations δ_h**. Amplitudes come from bilinear tables A_h(rpm, torque) extracted from real V8/I4 drives covering 0–7,007 rpm, including decel and idle. On top of that:
  - pink-noise AM of the harmonics with depth α to simulate "cycle-to-cycle combustion variability";
  - filtered-noise bursts AM'd by envelopes locked to orders 0.5/1.0/1.5/2.0;
  - a parallel Karplus-Strong resonator bank for exhaust resonances.
  - Analysis of the real recordings: "dominant 4th order at V8 firing frequency, **1.5th order during engine-braking**".
  - No perceptual evaluation — [arXiv 2603.07584](https://arxiv.org/html/2603.07584)
- A follow-up, "Gradient-Based Learning of Parametric Engine Sound Representations for Real-Time Resynthesis and Tuning on Embedded Systems" (arXiv 2606.21521, 2026), exists. I did not read it — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)

**Automotive ASD / EV engine-sound literature (perceptual findings)**
- Dupré, Denjean, Aramaki, Kronland-Martinet, "Analysis by synthesis of engine sounds for the design of dynamic auditory feedback of electric vehicles" (Acta Acustica 7:36, 2023). Their analysis of ICE sounds found **micro-modulations in both frequency and amplitude** and **resonances from engine-to-cabin transfer**. According to search-result summaries (I could not read the full text; 403), their perceptual test found that **including resonances significantly increased naturalness, while micro-modulations had no significant effect**. Summaries also report a "23% increase in pleasantness" for ICE-dynamics-linked feedback in a BEV — [Acta Acustica PDF](https://acta-acustica.edpsciences.org/articles/aacus/pdf/2023/01/aacus220112.pdf); [full HTML](https://acta-acustica.edpsciences.org/articles/aacus/full_html/2023/01/aacus220112/aacus220112.html)
- A 2025/26 deterministic-stochastic hybrid model (D-SHM) for EV ASD states that mainstream ASD synthesis is "relatively monotonous and exhibit[s] a distinctly artificial quality in actual listening tests". Its fix is a stochastic unit coupled with a deterministic order-synthesis unit — [ResearchGate](https://www.researchgate.net/publication/405716481_A_deterministic-stochastic_hybrid_sound_synthesis_method_for_enhancing_naturalness_of_active_sound_design_in_electric_vehicle)

### Inferences
- Across Baldan/SDT, engine-sim, PTR and Doerfler's dataset generator, **noise is never an independent additive layer**. It is gated by valve envelopes (SDT), multiplied into the pressure signal (engine-sim, §3), used as AM on the harmonics (Doerfler), or locked to the engine phase (PTR). If our "band-shaped flow noise" is summed beside the pulses with its own envelope, that is a likely cause of the "synthetic" timbre: the ear hears two streams instead of one fused source.
- Unequal header lengths and unequal firing phases (SDT `asymmetry`, ±k/12 header spread; PTR learned ±40° per-cylinder offsets; Jagla's full-cycle grains) all put energy into **half orders**. Perfectly identical cylinders give a pure firing-order comb that sounds buzzy and electronic, especially at idle where individual events are audible.
- Dupré et al.'s result (resonances matter, micro-modulation doesn't, *for EV feedback naturalness*) suggests that for a steady cabin-perspective sound the transfer path / resonance model matters more than random jitter. That was an EV-feedback context, though, with synthesized orders, not idle rumble. At idle, cycle-to-cycle variation is physically large, and the engine-sim and PTR evidence shows it matters there.

### Gaps
- No full-text access to Baldan 2015. I could not verify any listening-test numbers from it. The implementation details come from the SDT code, which may differ from the 2015 paper version.
- No full text for Jagla 2012, so how the enhanced PSOLA handles on/off load or deceleration is unverified. (It appears to be a single-recording, rpm-only method.)
- No full text of Dupré et al. 2023. The effect sizes, participant count and micro-modulation magnitudes are unverified.
- I found no published MOS/MUSHRA-style listening tests comparing procedural vs. sample-based engine synthesis for idle or deceleration specifically.
- GAN/diffusion engine-sound papers: I found none that are real-time and engine-specific with control inputs. Generic text-to-audio models were not investigated.

---

## 2. How idle and deceleration are handled per method

### Takeaway
Idle and deceleration are exactly where naive procedural synths fail, because both are dominated by **non-combustion or weak-combustion physics**: at idle, individual irregular firing events are audible; on overrun, pumping/motoring pulses and steady flow noise dominate, and the spectral balance shifts (e.g., strong half orders, loss of intake roar). The methods that handle decel well treat it as a **different source model**, not just the same pulses at lower gain.

### Cited Findings
- **SDT/Baldan.** Throttle multiplies only the spark burst. At throttle 0 the cylinder still emits `cos` compression pressure, and the valve-modulated waveguides still ring. So decel = motoring pulses plus resonances, with intake noise still gated by the valve. Backfire probability is armed only while rpm is falling and decays by factor `backfire` per pop — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
- **PTR (Doerfler & Wyse 2026).** Throttle uses a sublinear gain `torque^0.7`, so small throttle openings are audible, with a floor ε = 0.02 at idle. DFCO (negative torque) switches on a **steady turbulent aeroacoustic noise bank**, contrasted explicitly with "sharp rhythmical noise bursts" under throttle — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)
- **Real-data analysis:** a V8 shows a dominant 1.5th order during engine braking versus the 4th (firing) order under load — [arXiv 2603.07584](https://arxiv.org/html/2603.07584)
- **Game sample-based practice (Caviezel/BOOM):** off-throttle, "eliminate intake layer entirely" (the intake makes no sound off-throttle). Drop the engine layer several dB and EQ out the mid-lows. Reduce the exhaust and cut EQ around **2 kHz and 10 kHz**, plus attenuation **near the fundamental** — [BOOM Library / Mike Caviezel](https://www.boomlibrary.com/blog/the-car-engine-sound-primer-mike-caviezel/)
- **Caviezel on idle:** a dedicated idle sample plus samples every 500 rpm, **every 250 rpm at lower rpm**, because stretching pitch is most audible there. Crossfades are 250 rpm for the 500-rpm loops and 125 rpm for the tighter ones — [BOOM Library](https://www.boomlibrary.com/blog/the-car-engine-sound-primer-mike-caviezel/)
- **Forza (Turn 10, Nick Wiswell):** the audio uses "hundreds of physics based parameters", with rpm separately for engine/turbo/supercharger/transmission, plus throttle, load, torque, boost, clutch and gear. It explicitly addresses "how positive or negative engine load impact" the sound — [Designing Sound 2014](https://designingsound.org/2014/08/11/vehicle-engine-design-project-cars-forza-motorsport-5-and-rev/)
- **EngineLab (open source, C++/JUCE):** decel afterfire is "fed by unburned fuel on overrun and ignited by the pipe wall — which has to heat up first, exactly as on a real engine" — [EngineLab](https://github.com/zolaski333/EngineLab)
- **BeamNG:** `minLoadMix` / `maxLoadMix` set the blend range between off-load and on-load character. `offLoadGain` / `onLoadGain` set their levels. Separate `loadSmootherInRate` / `loadSmootherOutRate` and `rpmSmootherInRate` / `OutRate` "control chatter during quick throttle transitions". Afterfire is generated engine-side and emitted per outlet with its own volume, audio and muffling coefficients — [BeamNG docs](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/)

### Inferences
- For our synth:
  - Decel should switch the pulse source from "combustion pulse" (sharp attack, energy ∝ load) to "motoring/pumping pulse". That is a smaller, smoother, more sinusoidal compression/expansion pulse with no combustion transient; SDT's `cos` term with spark=0 is exactly this.
  - Crossfade in a **steady** (not pulse-gated) broadband flow noise that is modest and lowpassed.
  - Remove or strongly reduce any intake-roar component.
  - Attenuate near the firing fundamental while half-order/low-order content survives.
- Asymmetric smoothing of the load control, with a fast in-rate and a slower out-rate or vice versa as BeamNG allows, avoids an obviously "switched" on/off-load transition.
- Decel pops should be probabilistic, armed only on rpm-falling + closed throttle, and decay per event (SDT) or depend on exhaust temperature (EngineLab). A fixed-rate pop generator is an obvious synthetic cue.

### Gaps
- No quantitative listening data on which decel cues listeners use. Recommendations above are from practitioner advice and model design, not controlled tests.
- Idle cycle-to-cycle variation magnitudes (e.g., COV of IMEP at idle) were not in the sources read here; see the companion note `deceleration_physics.md` if it covers them.

---

## 3. Open-source physical simulators: engine-sim (AngeTheGreat) and EngineLab

### Takeaway
engine-sim does **not** use a clean pulse → waveguide chain. It simulates the gas dynamics of each cylinder and exhaust runner at a high rate (default audio input rate 10 kHz, adjustable "simulation frequency"). The per-cylinder exhaust pressure is delayed by pipe length, attenuated by 1/L² and summed. At audio rate it then **adds random fractional-delay jitter**, **multiplies the signal by lowpassed noise** (the flow-noise coupling), mixes in 1% of the time-derivative, and convolves with an **impulse response** (up to 10,000 samples), followed by an AGC leveler. The convolution step supplies most of the "real exhaust/mic" colour.

### Cited Findings (from the source code, master branch)
- **Audio source signal per cylinder:** `exhaustFlow = attenuation · 1600 · ((p_runner − 1 atm) + 0.1·dynP(+1) + 0.1·dynP(−1))`. This is the runner-and-primary static overpressure plus forward and backward dynamic pressure — [piston_engine_simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/piston_engine_simulator.cpp)
- **Per-cylinder propagation delay** is `(headerPrimaryLength + exhaustLength)/speedOfSound`, via a delay filter. The contribution is scaled by `soundAttenuation · audioVolume / cylinderCount · 1/(exhaustLength²)` and summed into one channel per exhaust system, so dual exhausts are separate channels — [piston_engine_simulator.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/piston_engine_simulator.cpp)
- **Default synthesizer parameters:** `inputSampleRate = 10000`, `audioSampleRate = 44100`, `convolution = 1.0`, `dF_F_mix = 0.01`, `inputSampleNoise = 0.5`, `inputSampleNoiseFrequencyCutoff = 10000`, `airNoise = 1.0`, `airNoiseFrequencyCutoff = 2000`, `levelerTarget = 30000`, `levelerMaxGain = 1.9` — [synthesizer.h](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/include/synthesizer.h)
- **Rate conversion:** the simulator output is linearly interpolated from the input rate to 44.1 kHz — [synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp)
- **Jitter filter.** It keeps a 10-sample history. Each output sample reads the history at a random fractional offset `s = lowpass_10kHz(uniform_noise · jitterScale(0.5))`, clamped to [0, 9], with linear interpolation. In effect the waveform is **read with a continuously wandering sub-millisecond time offset (≤10 samples ≈ 0.23 ms at 44.1 kHz)**. That is timing micro-jitter applied at sample level, not per event — [jitter_filter.h](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/include/jitter_filter.h)
- **Render chain per channel** — [synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp):
  1. `f = jittered − DC(10 Hz)`
  2. `f_p = d/dt(jittered)`
  3. `r = lowpass_2kHz(white)`
  4. `r_mixed = airNoise·r + (1−airNoise)`
  5. `v_in = f_p·0.01 + f·r_mixed·0.99`
  6. `v = conv·IR(v_in) + (1−conv)·v_in`
  7. The channels are summed, then pass a global lowpass at 0.45·fs, then a leveler (AGC), then int16 clipping.

  With the default `airNoise = 1`, **the pressure signal is fully amplitude-modulated by 2 kHz-lowpassed noise**. The noise exists only when and where there is pressure, so it is automatically synchronous with each exhaust pulse and scales with pulse strength.
- **Impulse responses** are clipped to at most 10,000 samples (≈0.23 s) — [synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp)
- **Runtime controls exposed in the UI:** volume, convolution level, high-frequency gain, low-frequency noise, high-frequency noise, and simulation frequency. The README calls it "NOT a scientific tool" — [engine-sim README](https://github.com/ange-yaghi/engine-sim)
- **Secondary description:** raw pressure-wave output "would sound thin and synthetic", and the convolution with impulse responses models exhaust acoustics and mic placement — [Starlog article](https://starlog.is/articles/developer-tools/ange-yaghi-engine-sim/). This is a secondary source; the code confirms convolution is on by default with `convolution = 1.0`.
- **EngineLab (zolaski333, C++20/JUCE):**
  - 0-D cylinders plus a conservative quasi-1-D duct network with characteristic waveguides, a thermal wall model and a passive radiation load at the outlet.
  - Exhaust elements include primaries, collectors, junctions, resonators, Munjal expansion chambers, Delany-Bazley porous packing, catalysts and multiple outlets.
  - Cycle-to-cycle variation is "emergent, not authored … with no authored random dispersion", arising from gas-dynamics, fuel-film, wave-action and ECU coupling.
  - It ships measurement harnesses: an audio render harness (RMS, crest, DC, per-band balance, exhaust-chain RT60), COV(IMEP) per cylinder, and an afterfire harness.
  - Limits: "audible propagation is linear", with no transverse modes or 3-D bends.

  — [EngineLab](https://github.com/zolaski333/EngineLab)

### Inferences
- There are four concrete differences between engine-sim and our architecture, and each is cheap to adopt:
  1. **Multiplicative, pulse-gated noise.** Use `out = pulse · (1 + k·LP(noise))` or engine-sim's full `pulse · LP(noise)` instead of `pulse + bandnoise`.
  2. **Continuous sample-level fractional-delay jitter** (≈0–10 samples at 44.1 kHz with a 10 kHz-smoothed random offset). It is not per-event timing randomization only; it roughens every waveform edge and kills the "perfect digital pulse" sound.
  3. **The pulse shape comes from the gas dynamics.** It includes reflections in the runner and dynamic pressure, so each pulse already carries secondary ripples. Smoothed analytic pulses lack this.
  4. **A final convolution with a real IR** (≤0.23 s). It is the single biggest "realism layer": it supplies the dense, irregular resonances of a real muffler, body and mic position that a 1D waveguide with a few reflections cannot.
- The 1/L² per-cylinder scaling plus per-cylinder delays for unequal header lengths reproduce the unequal-length "burble" naturally.
- The AGC leveler (max gain 1.9) keeps idle and decel audible by pushing quiet states up. The corollary is that engine-sim's idle loudness is not physically calibrated.

### Gaps
- Which IRs ship with engine-sim and how they were measured (real exhaust recordings vs. designed) was not verified in this pass.
- I did not verify engine-sim's internal fluid solver details (number of exhaust segments, timestep vs. "simulation frequency"), nor how it models misfire or idle instability.

---

## 4. Game/simulator industry and middleware approaches

### Takeaway
Shipping racing games are overwhelmingly **sample-based**. They use either (a) many crossfaded rpm loops per perspective and per load state, or (b) granular/per-cycle resynthesis from on-load and off-load ramp recordings (REV, AudioMotors, FMOD/Wwise granular plugins). Procedural parts are limited to layers and DSP: load EQ, afterfire, turbo, transmission whine. Pure granular interpolation is described by practitioners as "synthetic sounding in certain situations" and needs masking filters.

### Cited Findings
- **Project CARS (Stephen Baysted).** "Well over one hundred separate wave files" per car sound set, "layered and crossfaded and 'played' by physics inputs". Recordings are made on track or on a chassis dyno, with drivers following a specific set of tasks — [Designing Sound 2014](https://designingsound.org/2014/08/11/vehicle-engine-design-project-cars-forza-motorsport-5-and-rev/)
- **Forza Motorsport 5 (Nick Wiswell, Turn 10).** Built on FMOD Studio with complex signal flow, sends/returns and VCAs. It uses hundreds of physics parameters, including separate rpm for turbo/supercharger/transmission, plus positive and negative load — [Designing Sound 2014](https://designingsound.org/2014/08/11/vehicle-engine-design-project-cars-forza-motorsport-5-and-rev/)
- **REV (Crankcase Audio: Adam Boyd, John Twigg).**
  - It tracks the harmonic components of an engine ramp and slices it "at the granularity of individual pistons firing".
  - It changes speed by "skipping over cycles, or duplicating and repeating cycles".
  - It includes a gearbox model, virtual clutch and "layered load/offload mixing system".
  - Turnaround is ~10 minutes versus 3–4 days for loop sets.

  — [Designing Sound 2014](https://designingsound.org/2014/08/11/vehicle-engine-design-project-cars-forza-motorsport-5-and-rev/)
- **Greg Hill (AudioMotors / Soundwave Concepts).** Crossfaded loops remain "most widely used" because they keep "resonances, exhaust baffle vibrations, crackles and pops that would otherwise confuse and scramble the tracking within a granular synthesis method". Pure granular "interpolation is pretty synthetic sounding in certain situations", requiring "filters to mask these anomalies". His hybrid uses spectral analysis and automatic detection of engine-cycle crossfade points — [Designing Sound 2014](https://designingsound.org/2014/08/11/vehicle-engine-design-project-cars-forza-motorsport-5-and-rev/); [MCV/Develop](https://mcvuk.com/development-news/how-to-make-racing-car-engines-roar-using-audiomotors-fmod/)
- **Caviezel (BOOM Library primer).**
  - Loop approach: 13 loops across the rev range for a 5,500 rpm muscle car, × 4 perspectives (intake, engine, exhaust, interior) = 52 loops.
  - Granular alternative: 3 files (accel ramp, decel ramp, idle loop), with load switching handled by the granular engine, at a higher CPU and licensing cost.
  - Full throttle: optional distortion and low-end boost.

  — [BOOM Library](https://www.boomlibrary.com/blog/the-car-engine-sound-primer-mike-caviezel/)
- **BeamNG.drive.** Sample-based, with two continuous layers (engine/intake and exhaust/tailpipe) plus discrete layers: starter, shutoff, afterfire, turbo, supercharger, transmission, wind, tyre.
  - Load mix: `onLoadGain`/`offLoadGain` and `min`/`maxLoadMix`.
  - EQ: `eqFundamentalGain` (engine-order emphasis; negative reduces drone), plus low/high peaking and shelves.
  - Muffling: `intakeMuffling` and `exhaustAudioMufflingBaseCoef`, plus per-outlet muffling and gain.
  - `fundamentalFrequencyCylinderCount` must match the recorded engine or order emphasis is wrong.
  - rpm and load smoothers, and `cabinFilterCoef` for the interior.

  — [BeamNG docs](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/)
- **Wwise/granular:** Audiokinetic has a blog post "Engine Sound Modeling: From Sampling to Granular Synthesis in Wwise" (not readable, 403) — [Audiokinetic blog](https://www.audiokinetic.com/en/blog/engine-sound-modeling-from-sampling-to-granular-synthesis-in-wwise/). GDC talks exist on granular synthesis for games — [GDC Vault: Granular Synthesis in Next-Generation Games](https://gdcvault.com/play/1013205/Granular-Synthesis-in-Next-Generation); [GDC Vault: Data Driven Granular Synthesis for Video Games](https://www.gdcvault.com/play/1023987/Data-Driven-Granular-Synthesis-for) (paywalled, not read).

### Inferences
- The industry consensus explains our problem. Realism in shipping titles comes from **recorded resonances and irregularities**, which granular slicing tries to preserve per firing cycle. Where interpolation is procedural it sounds synthetic unless masked by filtering. A procedural synth therefore needs an equivalent of "recorded irregularity plus resonance": pulse-coupled noise, per-cylinder differences, and an IR/convolution stage.
- A pragmatic hybrid (the REV/Jagla idea) would drive per-cycle grains from our own procedural render or a small set of recorded cycles, repeating or skipping cycles to follow rpm. This keeps each 720° cycle's internal irregularity intact.

### Gaps
- No primary technical sources were found in this pass for Assetto Corsa/ACC, Automobilista 2/Madness, Gran Turismo, iRacing, rFactor or Codemasters engine audio internals. It is widely reported that ACC/AC use FMOD with sample banks, but I did not verify this here, so it is not cited.
- Sound Particles, SimSound, "Igniter" and Wwise SoundSeed Air were not researched. (SoundSeed Air is a wind/woosh generator, not an engine synth, from general knowledge only and not verified here.)
- GDC talk contents were paywalled.

---

## 5. Concrete tricks cited as critical for realism (implementation checklist)

### Takeaway
The recurring, source-backed ingredients are:
1. Noise coupled multiplicatively to the pulse or valve envelopes.
2. Sample-level fractional-delay jitter and per-cylinder asymmetry (unequal firing phase and header lengths).
3. A dense resonant transfer path: parallel detuned muffler waveguides, a Karplus-Strong bank, or better a convolution IR.
4. Operating-mode-specific sources: combustion bursts under load, motoring pulses plus steady flow noise on overrun.
5. DC blocking and an AGC/leveler.
6. Load-dependent EQ: off-load cuts near the fundamental and around 2 k/10 kHz, no intake.
7. Probabilistic, decaying decel pops.

### Cited Findings
- **Noise coupling:**
  - engine-sim: `v = pressure · LP2k(noise)` with airNoise = 1 — [synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp)
  - SDT: `intake = valve · LP(noise)` — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - Doerfler: pink-noise AM of the harmonics, plus filtered-noise bursts under envelopes locked to orders 0.5–2 — [arXiv 2603.07584](https://arxiv.org/html/2603.07584)
- **Timing jitter:**
  - engine-sim: ≤10-sample fractional delay driven by 10 kHz-lowpassed noise at scale 0.5 — [jitter_filter.h](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/include/jitter_filter.h)
  - Farnell: "four-stroke engine with jitter", with per-cylinder timing variation — [ASPress](https://aspress.co.uk/sd/practical22.html)
  - PTR: per-cylinder static offsets up to ±40° crank angle, learned — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)
- **Per-cylinder asymmetry:** in SDT, header/runner lengths are ×(1 ± k/12) and firing phases are offset by `0.5·asym·sin(2π·pos)/N` — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
- **Pulse shape:** PTR uses a bipolar derivative-of-cosines pulse with a `(1−e^{−αφ})e^{−βφ}` envelope, onset compression `φ^ν`, and harmonic rolloff `e^{−0.5kλ}` — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1). engine-sim mixes 1% of the time-derivative of pressure into the signal (`dF_F_mix = 0.01`) — [synthesizer.h](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/include/synthesizer.h)
- **Resonances:**
  - SDT: 4 parallel muffler waveguides at 0.7/0.9/1.1/1.3 × size, feedback 0.5 — [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
  - PTR: two-tap Karplus-Strong comb — [arXiv 2603.09391](https://arxiv.org/html/2603.09391v1)
  - engine-sim: IR convolution ≤10k samples — [synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp)
  - Dupré et al.: resonances significantly improved naturalness — [Acta Acustica](https://acta-acustica.edpsciences.org/articles/aacus/pdf/2023/01/aacus220112.pdf) (via search summary)
- **Off-load EQ/layering:** [BOOM Library](https://www.boomlibrary.com/blog/the-car-engine-sound-primer-mike-caviezel/); [BeamNG docs](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/)
- **Full-load distortion / low-end boost** as an optional layer — [BOOM Library](https://www.boomlibrary.com/blog/the-car-engine-sound-primer-mike-caviezel/)
- **Order-frequency micro-deviations** `f_h = (h+δ_h(t))f0`, measured from real engines — [arXiv 2603.07584](https://arxiv.org/html/2603.07584)
- **DC removal:** 10 Hz in engine-sim, 20 Hz default in SDT — [synthesizer.cpp](https://raw.githubusercontent.com/ange-yaghi/engine-sim/master/src/synthesizer.cpp); [SDTMotor.c](https://raw.githubusercontent.com/SkAT-VG/SDT/master/src/SDT/SDTMotor.c)
- **Control smoothing:** separate in and out rates for rpm and load — [BeamNG docs](https://documentation.beamng.com/modding/vehicle/sections/sounds/engine_audio/)

### Inferences (prioritized for our Rust synth; my judgement, not source claims)
1. **Replace additive band noise with pulse-multiplied noise** (engine-sim style `pulse·(a + b·LP(noise))`), and gate any intake noise by an intake-valve envelope. This is expected to be the largest idle-realism gain.
2. **Add a sample-rate fractional-delay jitter stage** (≤0.2 ms, smoothed noise) before the waveguide, in addition to the existing per-event randomization.
3. **Break cylinder symmetry deterministically:** per-cylinder header delays spread by ±5–10% and small fixed firing-phase offsets. That creates stable half-order content, which is what makes a real idle "lope" rather than buzz.
4. **Add a convolution stage with a real exhaust/mic IR** (≤0.25 s) after, or in place of, the tail of the 1D waveguide. Alternatively use a parallel bank of 3–4 detuned muffler waveguides or Karplus-Strong combs.
5. **Use a separate decel source model:**
   - motoring pulses (smooth `cos` compression, no combustion transient);
   - steady lowpassed flow noise, not pulse-gated;
   - no intake roar;
   - an attenuated fundamental;
   - pops armed on rpm-falling + closed throttle, with probability decaying per pop.
6. **Use a sublinear throttle→combustion gain** (`load^0.7`) with a non-zero idle floor, so small throttle changes are audible.

### Gaps
- No source gives validated numeric ranges for per-event amplitude/timing randomization at idle that listeners judge most natural. Values in the Inferences are starting points to tune by ear and analysis, not published optima.
- Cabin-perspective transfer (body panel modes, cabin IR) was not researched beyond BeamNG's `cabinFilterCoef` and Dupré et al.'s resonance finding.
