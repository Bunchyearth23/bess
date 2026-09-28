# Psychoacoustics of engine sound realism: cues, metrics and evaluation of synthesized engine sound

Scope: what listeners use to tell a real engine recording from a synthetic one, how to measure it objectively, and how to run a blind test. Written for a developer whose real-time procedural synth sounds artificial mostly at **idle** and during **deceleration/overrun**.

Source-quality note: several key papers were paywalled or bot-blocked (Acta Acustica / HAL / ScienceDirect / ResearchGate). Where only an abstract or search snippet was readable, this is stated. All arithmetic in the Inferences sections (order frequencies, binomial thresholds) was computed by the researcher and can be re-checked.

---

## 1. Sound-quality metrics and their perceptual meaning; typical values for real engines

### Takeaway
For idle, **loudness and sharpness matter most** for perceived quality, with **roughness and fluctuation strength** at roughly half their weight. Engine "roughness" comes mostly from **half-orders** (cylinder-to-cylinder differences) beating against the main orders. The only solid published reference values found are for idle cabin sounds of 32 cars: about 4.3 sone, 0.65 acum, 0.22 asper and 0.19 vacil on average. No citable per-operating-point tables (cruise, WOT, decel) were found.

### Cited Findings
- **Metric definitions:**
  - **Roughness** measures fast amplitude modulation in the 15–300 Hz range, heard as "harshness" or "beating". Engine booming and gear rattle often show up as higher roughness. **Fluctuation strength** measures slower variation below 20 Hz, heard as "wavering" or "pulsation", and is strongest near 4 Hz modulation. **Sharpness** measures how much the spectrum leans toward high frequencies. — [arXiv 2509.16901, EV psychoacoustic metrics](https://arxiv.org/pdf/2509.16901)
  - **Unit definitions:** 1 asper is the roughness of a 1 kHz, 60 dB tone, 100 % amplitude-modulated at 70 Hz. 1 vacil is the same tone modulated at 4 Hz. For a 1 kHz or higher carrier, roughness peaks at 70 Hz modulation. — [University of Salford, Roughness/fluctuation strength](https://acoustics.salford.ac.uk/psychoacoustics/sound-quality-making-products-sound-better/an-introduction-to-sound-quality-testing/roughness-fluctuation-strength/); [HEAD acoustics Application Note "Psychoacoustics II"](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf)
  - **Lower carriers:** for lower carrier frequencies the roughness peak moves to lower modulation frequencies. Roughness is also produced by frequency modulation, by amplitude-modulated noise, and by two tones within one critical band ("carrier-less" roughness). — [HEAD acoustics Psychoacoustics II](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf)
- **Half-orders cause engine roughness.** HEAD acoustics calls this "R-roughness" and says it is "particularly often perceived in the noise of combustion engines". It comes from half engine orders, which exist because "the combustion processes in the individual cylinders … are not exactly identical (e.g. variations in the intake system or the exhaust manifold)". Example given: a 4-cylinder engine at 3000 rpm has the 4th order at 200 Hz and the 3.5th and 4.5th orders at 175 and 225 Hz. How strong the half-order is sets the modulation depth. — [HEAD acoustics Psychoacoustics II](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf)
- **Reading roughness values:** in a roughness analysis, "the absolute values … should be considered less important than the ratio between different results". Real technical signals give smaller values than synthetic test tones. In their e-motor example the OK unit scored 0.048 asper and the reject 0.16 asper, a ratio of about 3. — [HEAD acoustics Psychoacoustics II](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf)
- **Tonality:** modern tonality is measured in tuHMS, using the Sottek hearing model, now an annex of ECMA-74. DIN 45681 is an alternative. — [HEAD acoustics Psychoacoustics II](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf)
- **Half-order perception (abstract/snippet level):**
  - Half-order components are perceived strongly because of the "fluctuation and muddiness" they produce, and they give a low-frequency "rumble" that lowers exhaust sound quality. — [Applied Acoustics, exhaust-manifold order distribution, 4-cyl](https://www.sciencedirect.com/science/article/abs/pii/S0003682X17309155) (abstract snippet only)
  - Roughness rises when half-integer-order harmonics are present. — [Camacho et al. 2008, Acta Acustica united with Acustica, "Exploring roughness perception in car engine noises through complex cepstrum analysis"](https://www.researchgate.net/publication/233658577_Exploring_Roughness_Perception_in_Car_Engine_Noises_through_Complex_Cepstrum_Analysis) (snippet only)
  - Crankshaft "rumble" modulates at one-half order, which points to a single cylinder or main bearing. — [Experimental characterization of engine crankshaft rumble noise signatures](https://www.researchgate.net/publication/238946668_Experimental_characterization_of_engine_crankshaft_rumble_noise_signatures) (snippet)
  - A single-cylinder misfire gives a strong half-order speed variation. — [US patent 7292933, misfire detection](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7292933) (snippet)
  - 0.5-order excitation is a recognised NVH issue in inline-4 engines. — [SAE 962492](https://saemobilus.sae.org/content/962492)
- **Measured idle values (Altinsoy 2013, Int. J. Vehicle Noise and Vibration):**
  - Stimuli were binaural cabin recordings of **32 cars at idle**.
  - Averages were **loudness 4.3 sone** (ISO 532B / Zwicker), **sharpness 0.65 acum** (Aures), **roughness 0.22 asper** (Aures), **fluctuation strength 0.19 vacil** (Terhardt) and **relative approach 3.1 cPa** (Genuit/Sottek).
  - The quality index `QI = L + 6·S + 10·R + 10·FS + RA` explains **r² = 0.86** of the quality ratings. Level alone explains much less: r² = 0.44 for dB and 0.53 for dB(A).
  - "Loudness and sharpness are almost equally important". Roughness and fluctuation strength weigh about half as much.
  - "Not only the sound levels but also their modulations are important. Terms such as 'diesel' and 'vibration' can be described using the modulation properties at high frequencies." Relative approach "is able to detect rapid time varying signal structures (e.g. diesel knocking)".
  - Source: [Altinsoy 2013, TU Dresden PDF](https://tu-dresden.de/ing/elektrotechnik/ias/aha/ressourcen/dateien/professur/publikationen/Altinsoy2013h_-_Identification_of_Quality_Attributes_of_Automotive_Idle_Sounds_and_Whole-Body_Vibrations.pdf?lang=en)
- **Idle vocabulary (same study):**
  - Listeners' own words for idle sounds were **humming, rough, wavy, bumpy, clatter**, plus booming, roaring, rattling, whistling and "diesel".
  - Low-frequency engine-order components drive "humming" and "booming".
  - Ratings of "aggressive" correlated with "booming" (r² = 0.75) and "roaring" (r² = 0.72). "Powerful" correlated with "roaring" (r² = 0.80).
  - Source: [Altinsoy 2013](https://tu-dresden.de/ing/elektrotechnik/ias/aha/ressourcen/dateien/professur/publikationen/Altinsoy2013h_-_Identification_of_Quality_Attributes_of_Automotive_Idle_Sounds_and_Whole-Body_Vibrations.pdf?lang=en)

### Inferences
- **Half-orders at idle land in the fluctuation-strength band, not the roughness band.** For an inline-4 at 800 rpm: rotation (order 1) is 13.3 Hz, firing (order 2) is 26.7 Hz, and order 0.5 is 6.7 Hz. A 0.5-order or 1st-order imbalance therefore modulates the firing pulse train at 6.7 or 13.3 Hz, which is the "wavy/bumpy/lumpy idle" character. At 3000 rpm the same imbalance modulates at 25 Hz and becomes roughness. A synth that fires every cylinder identically has **no half-orders**. Its idle will then score near-zero fluctuation strength and sound like a buzzer.
- **Use Altinsoy's averages as a sanity range for cabin idle,** not as a target. Synthetic idle far below about 0.1 vacil or about 0.1 asper at cabin-like levels suggests missing modulation. Compare against a real recording using ratios, as HEAD advises, rather than absolute values.
- **"Diesel"-like clatter is a high-frequency modulation cue.** Diesel clatter is broadband high-frequency energy modulated at firing rate. Steady high-frequency noise that is not synchronised to the cycle will not produce it.

### Gaps
- No citable table of loudness, sharpness, roughness, fluctuation strength and tonality for the same engine at idle, cruise, WOT and overrun was found. Zwicker & Fastl, HEAD acoustics and Genuit sources with such data were not accessible. Measure these on your own reference recordings instead. MATLAB `acousticRoughness`, `acousticFluctuation` and `acousticSharpness` implement the standard models ([MathWorks psychoacoustics](https://www.mathworks.com/help/audio/psychoacoustics.html)).
- No quantitative "booming" metric definition was found. Booming is usually tied to low-frequency order levels in the 30–150 Hz range, but that range is industry lore and was not verified here.

---

## 2. Periodicity and jitter: how much cycle-to-cycle variation sounds natural

### Takeaway
Voice synthesis clearly shows that **perfectly periodic sources sound artificial** and that **amplitude and timing perturbation together** make them natural. The voice literature gives thresholds of about 0.5–1 % period jitter and 1–4 % amplitude shimmer. **No engine-specific jitter/shimmer perception study was found.** The one engine-related ablation (Dupré et al. 2023) found that adding micro-modulations did *not* significantly raise naturalness, while adding cabin resonances did. Treat jitter as necessary but not sufficient.

### Cited Findings
- **Jitter and shimmer together made synthetic vowels pass as human** (Yamasaki et al. 2017, J. Voice):
  - Three judges labelled 120 vowels as human or synthesized. All human vowels were labelled human.
  - **27 of 80 synthesized vowels passed as human.** Of these, 15 had jitter plus shimmer, 10 had jitter only, 2 had no perturbation and **0 had shimmer only**.
  - In a ranking task, jitter plus shimmer was rated most natural. Shimmer-only and unperturbed vowels were rated most artificial.
  - Source: [Yamasaki et al. 2017 (Europe PMC / PubMed 27777057)](https://pubmed.ncbi.nlm.nih.gov/27777057/)
- **Rozsypal & Millar (1979, J. Phonetics):** in paired comparisons of synthetic vowels, "some degree of roughness is necessary for synthetic vowels to be perceived as natural". No trade-off between jitter and shimmer was found. — [ScienceDirect abstract](https://www.sciencedirect.com/science/article/pii/S0095447019310691) (abstract/snippet only)
- **Voice perturbation reference values (MDVP pathology thresholds, as documented in Praat):**
  - Jitter (local) 1.040 %; absolute 83.2 µs; RAP 0.680 %; PPQ5 0.840 %.
  - Shimmer (local) 3.810 %; 0.350 dB; APQ11 3.070 %.
  - Praat notes that the jitter thresholds were inflated by noise, so "the correct threshold is probably lower".
  - Sources: [Praat manual, Voice 2. Jitter](https://www.fon.hum.uva.nl/praat/manual/Voice_2__Jitter.html); [Praat manual, Voice 3. Shimmer](https://www.fon.hum.uva.nl/praat/manual/Voice_3__Shimmer.html)
- **Unverified voice figures:** a search summary said jitter of 1–2 % is typical of modal voice, shimmer JND for modal voice is 1–4 %, and synthesis shimmer "up to 10 %". These are attributed to a 2025 review by Koffi ([St Cloud State repository](https://repository.stcloudstate.edu/stcloud_ling/vol14/iss1/2/)), which could not be opened to verify them.
- **Engine-specific result (Dupré, Denjean, Aramaki, Kronland-Martinet 2023, Acta Acustica, DOI 10.1051/aacus/2023031):**
  - Time-frequency analysis of combustion-engine cabin sounds showed **micro-modulations in both frequency and amplitude** and **resonances from engine-to-cabin sound transfer**.
  - Both were added to a Shepard-Risset EV sonification model.
  - Perceptual test: "the inclusion of resonances … significantly enhanced their naturalness, while micro-modulations had no significant impact."
  - Source: [Acta Acustica full text](https://acta-acustica.edpsciences.org/articles/aacus/full_html/2023/01/aacus220112/aacus220112.html). Only the abstract, via Crossref, was readable; the micro-modulation depths and rates used are not known.
- **Random-walk "humanizing" of a physically inspired motor model** (Hendry & Reiss 2010, as summarised in af Malmborg 2021):
  - A random walk on max speed, brush-noise level and three EQ band gains was credited as "a contributing factor to the perceived realness".
  - Spectra matched the recordings closely, but zero-crossing rate did not.
  - Source: [af Malmborg 2021, LTU thesis](https://ltu.diva-portal.org/smash/get/diva2:1557027/FULLTEXT01.pdf)
- **Combustion variability in real engines:**
  - Cycle-to-cycle combustion variability (measured as COV of IMEP) is studied specifically at idle. — [Springer JMST, cyclic IMEP variation at idle](https://link.springer.com/article/10.1007/BF03184801)
  - Heavy spark retard raises that variability, and calibrations cap COV-IMEP partly to avoid NVH problems. — [ORNL, combustion variability for aftertreatment light-off](https://impact.ornl.gov/en/publications/modelling-and-estimation-of-combustion-variability-for-fast-light/)
  - No numeric COV values were readable in either source.
- **Idle speed stability:**
  - Idle control studies report closed-loop speed fluctuation of **±50 rpm (PID) vs ±25 rpm (fuzzy)** on a 4-cylinder gasoline engine at no load. — [Atlantis Press review, engine idle speed control](https://www.atlantis-press.com/article/25844909.pdf) (snippet)
  - A low-quality consumer page claims ±10–15 rpm over 60 s for a "stable" idle. — [alibaba product insights](https://www.alibaba.com/product-insights/how-many-revs-should-a-car-idle-at-typical-rpm-range-explained.html) (treat as unreliable)
- **1/f versus white fluctuation:** in melodic sequences, "white noise sequences sound too random … brown noise sequences sound too predictable … pink noise … perceived as musical". — [Wikipedia: Pink noise](https://en.wikipedia.org/wiki/Pink_noise); [J.O. Smith, CCRMA, 1/f noise synthesis](https://ccrma.stanford.edu/~jos/sasp/Example_Synthesis_1_F_Noise.html). This refers to the Voss & Clarke melody experiments, which are not about engines.

### Inferences
- **Suggested parameterisation, to verify by ear** (engine values extrapolated from voice data, not measured on engines):
  - **Per-cycle timing jitter:** start at about 0.3–1 % of the cycle period. At 800 rpm the 4-stroke cycle is 150 ms, so 0.5 % is about 0.75 ms. Make it **low-pass correlated** (AR(1) or 1/f), not i.i.d. white per cycle, so that rpm wanders slowly, as a speed governor holding ±25 rpm would.
  - **Per-cylinder, per-event amplitude:** give each cylinder a fixed offset of a few percent. This creates the half-orders (see section 1).
  - **Per-event random scatter:** a few percent on top of the fixed offset. This models combustion variability.
  - **Why both:** the Yamasaki result (shimmer alone is worst; jitter plus shimmer is best) argues for varying timing and amplitude together, not amplitude alone.
- **Uncorrelated white per-cycle randomisation is a likely "too random / gravelly" giveaway.** Real speed fluctuation is filtered by crankshaft inertia and the idle controller, so it is correlated across cycles. Per-cylinder offsets are **deterministic and periodic** over the 720° cycle, not random.
- **The null Dupré result on micro-modulations does not license leaving them out.** Their stimulus was an EV Shepard-Risset sound, not a firing-pulse engine model. It does suggest that **transfer-path resonances** (formant-like fixed filters for exhaust, intake and cabin) are perceptually more important than modulation fine-tuning.

### Gaps
- No engine study measuring perceived naturalness as a function of cycle-to-cycle jitter or shimmer was found. Nor was a measured jitter percentage for real engine firing intervals at idle.
- Rozsypal & Millar's actual jitter/shimmer levels and Dupré's micro-modulation parameters were not accessible.

---

## 3. Studies comparing synthetic and recorded engine sounds: what gives synthesis away

### Takeaway
The clearest recent evidence: a **purely harmonic (engine-order) synth** scores about 35/100 for realism against about 61–76 for real recordings. **Adding learned broadband/stochastic noise bands** closes the gap for naive listeners and nearly closes it for experts. Other documented giveaways are loop and pitch-shift artifacts (clicks and repetition), unnatural pitch trajectories, a dull or low-passed spectrum, and missing transfer-path resonances.

### Cited Findings
- **Doerfler, Kuntz, Zimmer (Impulse Audio Lab), arXiv 2606.21521, June 2026:**
  - **Model:** harmonic orders `m ∈ {0.5, 1.0, 1.5, …}`, K_h = 64 orders **including half-orders**, plus an **ERB-spaced filtered noise bank** (random-phase IFFT bands). Amplitudes are factorised into separate **RPM gain curves × torque gain curves**, so timbre depends on load.
  - **Listening test:** 12 participants (6 expert, 6 naive) answered "How realistic does this stimulus sound?" on a 0–100 slider, reset to a random position after each playback, with 3 repetitions.
  - **Scores:**

    | Listeners | Harmonic-only (EOE) | Harmonic + noise (EONE) | Real recording (REC) |
    |---|---|---|---|
    | Naive | 35 | 61 | 61 |
    | Expert | 35 | 71 | 76 |

  - **Significance:** EONE vs REC was not significant overall (p = 0.13), but was significant for experts (p < 0.03).
  - **Authors' view:** purely harmonic representations have "insufficient acoustic realism", and stochastic components are "the primary driver of this improvement".
  - **Objective metric and loss:** log-spectral distance (STFT 4096, hop 512, bins below 10 Hz discarded); multi-resolution STFT loss (FFT 32768 down to 32) plus a harmonic loss along the order tracks.
  - **Public recordings:** they use a "Procedural engine sounds dataset" on Zenodo, DOI 10.5281/zenodo.16883336.
  - Source: [arXiv 2606.21521](https://arxiv.org/html/2606.21521)
- **Dupré et al. 2023:** users said EV sonification sounds "do not blend seamlessly with the other sounds in the vehicle cabin". Engine-to-cabin **resonances** significantly improved naturalness; micro-modulations did not. — [Acta Acustica](https://acta-acustica.edpsciences.org/articles/aacus/full_html/2023/01/aacus220112/aacus220112.html)
- **Loop and wavetable artifacts:** "the onset and offset of each sound will be audibly identifiable when the sounds are played in a loop, which results in repeated clicking", caused by "the discontinuity of the phase between the boundaries of two sound samples". — [Real-time automotive engine sound simulation with DNN (Duke Kunshan)](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf)
- **Game-audio comparison (af Malmborg 2021, Luleå bachelor thesis):**
  - Compared a sample/loop blend model, a granular model (Crankcase REV, Porsche 997 preset) and a physical model.
  - **Granular was rated most realistic, the physical model least preferred.**
  - REV exists to avoid "the artificial sound that usually occurs when using a traditional loop-based method of pitch-shifting recordings".
  - Participant comments on the weakest model: "sounds synthetic and low passed", "artificial in the way that its pitch rose", "like a very old game or a parody".
  - Source: [af Malmborg 2021](https://ltu.diva-portal.org/smash/get/diva2:1557027/FULLTEXT01.pdf)
- **GTA V:** uses a granular engine with extra oscillators synced to the granular clock to add sub-harmonics. — [af Malmborg 2021, citing MacGregor 2014](https://ltu.diva-portal.org/smash/get/diva2:1557027/FULLTEXT01.pdf)
- **Across procedural sound effects** (Moffat & Reiss 2018, as summarised): "in 4 out of 7 cases, procedural synthesis engines generated sounds that were indistinguishable, in terms of perceived realism, from recorded samples". Many papers lack perceptual evaluation. — [af Malmborg 2021](https://ltu.diva-portal.org/smash/get/diva2:1557027/FULLTEXT01.pdf); [State of the Art in Procedural Audio](https://www.academia.edu/143297003/The_State_of_the_Art_in_Procedural_Audio)
- **Per-cycle recorded grains:** Jagla, Maillard & Martin (JASA 2012) extract **one engine cycle per grain**, located from engine periodicity, from a run-up recording. They concatenate grains with PSOLA-style overlap-add and report "high quality audio restitution with a low computational load". — [JASA 132(5):3098](https://pubs.aip.org/asa/jasa/article-abstract/132/5/3098/854169/Sample-based-engine-noise-synthesis-using-an)
- **Classic physically inspired recipe:** earlier approaches sum harmonics for "mechanical" sound and add random noise matched to measured spectra for combustion noise. — [Duke Kunshan DNN paper, related work](https://sites.duke.edu/dkusmiip/files/2023/12/Engine_Sound_Synthesis_ncmmsc-52-1.pdf); Baldan et al. 2015 physically informed model ([IEEE SIVE 2015, ResearchGate](https://www.researchgate.net/publication/280086598_Physically_informed_car_engine_sound_synthesis_for_virtual_and_augmented_environments); evaluation details not accessible)
- **Procedural foley in general:** believable for sci-fi-like sounds (lasers, rockets), less so for everyday sounds. — [arXiv 2605.31082](https://arxiv.org/pdf/2605.31082)

### Inferences
Ranked list of likely giveaways for a procedural synth, strongest evidence first:
1. **Missing or incorrect broadband (stochastic) component.** This is the largest measured effect: 35 → 61–71. The noise must be **modulated synchronously with the firing cycle**, not a steady hiss.
2. **Missing half-orders and cylinder imbalance** (section 1).
3. **Missing transfer-path resonances** (Dupré): fixed formant-like peaks from exhaust pipes, muffler and cabin that stay put while the orders sweep through them.
4. **Timbre that does not change with load.** Doerfler needed separate RPM and torque gain curves. Overrun, which is low or negative torque, must sound different from constant-speed idle at the same rpm.
5. **Pitch trajectory artifacts:** unnaturally smooth or linear rpm ramps ("artificial in the way its pitch rose").
6. **Discontinuities:** clicks at loop or grain boundaries and zipper noise on parameter steps.
7. **Dull, low-passed spectra.**

- For a from-scratch procedural synth, the Doerfler architecture is a good template: half-orders plus about 64 orders plus an ERB noise bank, with rpm × torque gain curves. Their LSD and multi-resolution STFT metrics are a ready objective proxy.

### Gaps
- No study was found that isolates phase coherence (e.g. random vs locked order phases), aliasing, or zipper noise as realism cues for engines specifically. These remain plausible, not demonstrated.
- No listening study focused specifically on **idle** or **overrun** realism of synthetic engines was found.

---

## 4. Role of the listening environment: mono vs stereo, room, distance, cabin filtering

### Takeaway
Cabin/transfer-path resonances measurably increase naturalness (Dupré 2023). Automotive jury testing uses **equalised binaural playback over headphones**, which correlates well with in-car judgments. Giving listeners context (a ride in the car, a video) **reduces rating variance**. No study quantifying mono vs stereo vs binaural *realism* for synthetic engines was found.

### Cited Findings
- Engine-to-cabin transfer resonances significantly improved naturalness of synthesized vehicle sounds. — [Dupré et al. 2023](https://acta-acustica.edpsciences.org/articles/aacus/full_html/2023/01/aacus220112/aacus220112.html)
- Altinsoy's idle study played **binaural recordings through HEAD acoustics HA II.1 headphones with a PEQ IV equaliser**. It used videos of starting and idling a car to give listeners context. — [Altinsoy 2013](https://tu-dresden.de/ing/elektrotechnik/ias/aha/ressourcen/dateien/professur/publikationen/Altinsoy2013h_-_Identification_of_Quality_Attributes_of_Automotive_Idle_Sounds_and_Whole-Body_Vibrations.pdf?lang=en)
- Road-test judgments and playback over loudspeakers and headphones (dummy-head recordings) were found to be highly correlated. — [Farina & Ugolotti, "Subjective evaluation of the sound quality in cars by the auralisation technique" (Reproduced Sound 1997)](https://www.researchgate.net/publication/2458446_Subjective_Evaluation_Of_The_Sound_Quality_In_Cars_By_The_Auralisation_Technique) (snippet)
- **Context effect:** average ratings were mostly unchanged, but variances were significantly smaller when subjects had ridden in the car before the lab test. — [Sound quality evaluation of electric cars: preferences and influence of the test environment](https://www.researchgate.net/publication/270448622_Sound_Quality_Evaluation_of_Electric_Cars_-_Preferences_and_Influence_of_the_Test_Environment) (snippet)
- Doerfler et al.'s recordings came from differing environments and operating-mode coverage "representative of practical real-world data conditions". — [arXiv 2606.21521](https://arxiv.org/html/2606.21521)

### Inferences
- **Compare like with like in a blind test.** A dry, mono, anechoic-sounding synth set against a cabin or outdoor recording will be identified by its **acoustic space**, not its engine model. Either render the synth through the same kind of space (cabin IR or exterior with ground reflection) or pick references recorded close-mic'd and dry.
- **Stereo decorrelation matters.** Real stereo or binaural recordings have left/right channels that are only partly correlated (reflections, separate exhaust tips, intake vs exhaust). A synth that sends identical mono to both channels may sound "inside the head" and artificial on headphones. This is the researcher's hypothesis; no engine study supports it.

### Gaps
- No quantitative data found on how much mono vs stereo, reverb or distance cues shift realism ratings of engine sounds.

---

## 5. Evaluation methodology: ABX, MUSHRA, paired comparison, semantic differential, objective proxies

### Takeaway
To answer "is it indistinguishable?", use a **forced-choice discrimination test (ABX or 2AFC "which one is real?")** and a binomial criterion. To answer "how realistic is it?", use a **0–100 realism slider in MUSHRA style** with a hidden real reference. **Semantic differentials** diagnose *why* it differs. Objective proxies are log-spectral distance, multi-resolution STFT distance, order-tracked levels, the modulation spectrum and psychoacoustic metrics.

### Cited Findings
- **MUSHRA (ITU-R BS.1534-3):** multiple stimuli plus a hidden reference plus anchors (typically 3.5 kHz and 7 kHz low-pass), rated 0–100. Listeners who rate the hidden reference below 90 for more than 15 % of items are excluded. It suits intermediate-quality differences; ITU-R BS.1116 is recommended for small impairments. — [Wikipedia: MUSHRA](https://en.wikipedia.org/wiki/MUSHRA); [FORCE Technology BS.1534-3 sheet](https://forcetechnology.com/-/media/force-technology-media/pdf-files/unnumbered/senselab/product-sheet-itu-r-bs,-d-,1534-3-mushra.pdf)
- MUSHRA-style tests are less able to show significance for subtle differences, while A/B tests can expose them with fewer participants. — [arXiv 2203.04444, Reproducible Subjective Evaluation](https://arxiv.org/pdf/2203.04444) (snippet)
- **Realism-slider protocol used successfully for engines:** 0–100 "How realistic does this stimulus sound?", slider reset to random, 3 repetitions averaged, expert and naive groups analysed separately. — [Doerfler et al. 2026](https://arxiv.org/html/2606.21521)
- **Semantic differential for idle** (Altinsoy):
  - 38 naive native-German listeners.
  - Attributes were elicited by free verbalisation plus the Repertory Grid Technique and reduced to about 22–34 adjectives.
  - Factor analysis gave 4 factors explaining 93 % of variance. The first factor (pleasant, comfortable, aggressive, annoying, strained …) explains 48 %.
  - Source: [Altinsoy 2013](https://tu-dresden.de/ing/elektrotechnik/ias/aha/ressourcen/dateien/professur/publikationen/Altinsoy2013h_-_Identification_of_Quality_Attributes_of_Automotive_Idle_Sounds_and_Whole-Body_Vibrations.pdf?lang=en)
- **Human vs synthesized labelling:** Yamasaki et al. used exactly this binary task, with 20 % repeated items to check consistency. — [Yamasaki 2017](https://pubmed.ncbi.nlm.nih.gov/27777057/)
- **Objective proxies used in the literature:**
  - Log-spectral distance, multi-resolution STFT loss and harmonic (order-track) energy loss. — [Doerfler 2026](https://arxiv.org/html/2606.21521)
  - Loudness, sharpness, roughness, fluctuation strength and relative-approach regression. — [Altinsoy 2013](https://tu-dresden.de/ing/elektrotechnik/ias/aha/ressourcen/dateien/professur/publikationen/Altinsoy2013h_-_Identification_of_Quality_Attributes_of_Automotive_Idle_Sounds_and_Whole-Body_Vibrations.pdf?lang=en)
  - Modulation analysis of depth vs carrier and modulation frequency (e.g. HEAD ArtemiS modulation analysis). — [HEAD Psychoacoustics II](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf)
  - Zero-crossing rate, as a mismatch indicator. — [af Malmborg 2021](https://ltu.diva-portal.org/smash/get/diva2:1557027/FULLTEXT01.pdf)

### Inferences
**Quick informal blind test for one developer**, designed by the researcher from the cited protocols:

1. **Stimuli:** pairs of real and synth clips of 4–8 s at matched operating points: idle 30 s steady (cut into segments), tip-out from 3000–4000 rpm to idle, and a slow rev.
   - Loudness-match with ITU-R BS.1770 LUFS or Zwicker loudness to within ±0.5 dB, because level is a strong cue.
   - Match bandwidth, sample rate and acoustic space.
   - Randomise the start offsets so the same real segment is not always used.
   - Apply 20–50 ms raised-cosine fades.
2. **Task:** 2AFC "which is the real one?" or ABX. Use at least 16 trials per condition, randomised order and a blind tool such as foobar2000 ABX, lossyWAV ABX, webMUSHRA or a small script.
3. **Criterion** (one-sided binomial, p < 0.05, computed by the researcher):

   | Trials | Correct needed | p |
   |---|---|---|
   | 10 | ≥ 9 | 0.011 |
   | 12 | ≥ 10 | 0.019 |
   | 16 | ≥ 12 | 0.038 |
   | 20 | ≥ 15 | 0.021 |
   | 24 | ≥ 17 | 0.032 |
   | 32 | ≥ 22 | 0.025 |

   "Indistinguishable" means that over several listeners, including at least one expert, pooled scores are not significantly above 50 %. Experts discriminate better (Doerfler), so recruit someone who knows engines.
4. **Graded realism:** a MUSHRA-style page with a hidden real reference and a "bad synth" anchor (e.g. harmonic-only version) on a 0–100 realism slider, following Doerfler's protocol. Track the synth score relative to the hidden real score across builds.
5. **Diagnosis:** after each discrimination trial, ask "what gave it away?" with checkboxes: too clean/buzzy, too steady, too random/gravelly, missing rumble, pitch movement, clicks/zipper, dull/low-passed, wrong space. This is a lightweight semantic differential.

**Objective proxies to automate, synth vs a real reference at the same operating point:**
- **(a) Order-tracked levels** of orders 0.5 to about 32, in dB relative to the firing order, including the half-orders.
- **(b) Stochastic-to-harmonic ratio per ERB band.** Remove the order components by comb or order filtering and compare the residual spectra.
- **(c) Envelope modulation spectrum per critical band**, over 0.5–300 Hz modulation frequency. This is the direct objective counterpart of fluctuation strength and roughness, and the best single "sounds mechanical" detector.
- **(d) Per-cycle period series:** from crank-synchronous peak picking, compute jitter %, per-cycle energy (shimmer %) and their **autocorrelation or spectral slope** to test white vs 1/f.
- **(e) Log-spectral distance and multi-resolution STFT distance** as in Doerfler.
- **(f) Loudness, sharpness, roughness and fluctuation-strength ratios**, synth over real.

### Gaps
- No published ABX study of synthetic vs real engine sounds was found. Doerfler used ratings, not forced choice.

---

## 6. Practical checklist: signal signatures of a real engine at idle and on overrun

### Takeaway
A real idle is a **cycle-synchronous pulse train with cylinder-to-cylinder imbalance (half-orders), slow correlated speed wander, firing-synchronous broadband noise, and fixed transfer-path resonances**. Overrun, typically with fuel cut, removes most combustion excitation and leaves a **load-dependent timbre change**, plus optional **stochastic exhaust pops**. Several items below are derived from the cited mechanisms rather than measured directly.

### Cited Findings
- Half-orders exist because individual cylinders' combustion and intake/exhaust paths differ. They are the main source of engine roughness and, at low rpm, of "fluctuation/muddiness/rumble". — [HEAD Psychoacoustics II](https://cdn.head-acoustics.com/fileadmin/data/global/Application-Notes/SVP/Psychoacoustic-Analyses-II_e.pdf); [Applied Acoustics exhaust manifold study](https://www.sciencedirect.com/science/article/abs/pii/S0003682X17309155)
- Idle vocabulary is humming, rough, wavy, bumpy, clatter. Modulations, not just levels, drive idle quality. Diesel clatter and knock are rapid time-varying structures, which relative approach captures. — [Altinsoy 2013](https://tu-dresden.de/ing/elektrotechnik/ias/aha/ressourcen/dateien/professur/publikationen/Altinsoy2013h_-_Identification_of_Quality_Attributes_of_Automotive_Idle_Sounds_and_Whole-Body_Vibrations.pdf?lang=en)
- Broadband stochastic components are essential for realism. Timbre varies jointly with rpm and torque. — [Doerfler 2026](https://arxiv.org/html/2606.21521)
- Engine-to-cabin resonances raise naturalness. — [Dupré 2023](https://acta-acustica.edpsciences.org/articles/aacus/full_html/2023/01/aacus220112/aacus220112.html)
- Idle speed control holds rpm only to within about ±25–50 rpm in the cited control studies. — [Atlantis Press idle control review](https://www.atlantis-press.com/article/25844909.pdf)
- **Overrun mechanics:**
  - On lift-off the throttle closes and the ECU usually applies **deceleration fuel cut-off (DFCO)**. Pops and crackles come from unburnt fuel igniting in the hot exhaust, deliberately enhanced by injecting fuel and/or retarding ignition on overrun. — [EngineerFix](https://engineerfix.com/what-makes-a-car-exhaust-pop-and-crackle/); [ThunderMax "Deceleration pop problem"](https://www.thunder-max.com/techdocs/DecelPop1.pdf); [ZipTuning pop & bang](https://www.ziptuning.com/tuning-file/pop-bang/)
  - These are non-academic sources.

### Inferences
Researcher-derived checklist; each item is a testable signal property.

**Idle**
1. **Firing order and half-orders.**
   - The firing order (order 2 for an inline-4, 3 for an inline-6, 4 for a V8) should be clearly dominant.
   - Visible **0.5, 1, 1.5 … orders** should sit some dB below it. Measure the level on your reference; do not guess.
   - If the synth's order spectrum shows only multiples of the firing order, it will sound like a buzzer.
2. **Per-cylinder signatures.** Each cylinder has slightly different pulse amplitude, shape and timing, fixed over the 720° cycle. This produces 4–7 Hz "wavy/bumpy" fluctuation at 800 rpm for an inline-4.
3. **Slow, correlated rpm wander.** Roughly ±10–50 rpm, with most energy below a few Hz and a 1/f- or AR-like spectrum, not i.i.d. per cycle. Occasional larger events (A/C compressor, alternator load) cause a small dip and recovery.
4. **Per-event combustion scatter.** A few percent of pulse amplitude, and optionally small pulse-shape variation, uncorrelated between events but on top of the deterministic per-cylinder offsets.
5. **Firing-synchronous broadband noise.** Combustion, valvetrain ticking and injector clicks, amplitude-modulated at firing rate and cam rate. The envelope modulation spectrum of high-frequency bands should show peaks at firing frequency and its harmonics. Diesel "clatter" is exactly this at high frequency.
6. **Mechanical tonal components not locked to firing.** Accessories and gear/chain whine may be fixed-ratio orders with non-integer multipliers.
7. **Fixed resonances.** Exhaust pipe, muffler and cabin modes act as formants: the peaks stay at constant frequency when rpm changes.
8. **No perfectly stable waveform.** Measure autocorrelation between successive cycles. Real recordings will show clearly below 1.0; a naive synth will show about 1.0.

**Overrun / deceleration**
1. **Timbre change at tip-out, not a pitch ramp on the idle or cruise timbre.** Under DFCO combustion pulses mostly disappear, so the level of low-order combustion content drops sharply. What remains is compression/pumping pulsation, intake noise, mechanical and valvetrain noise, and exhaust resonances. Model this as a separate torque-dependent gain curve (Doerfler's rpm × torque factorisation).
2. **Non-linear rpm decay.** The trajectory follows drivetrain load and gear. In neutral there is a free-rev decay and an idle-controller catch with a small undershoot. Avoid linear ramps ("artificial in the way its pitch rose").
3. **Optional stochastic pops.** Sparse, irregular, broadband impulses, heavy on low-mid energy with a sharp attack, loosely tied to exhaust events. Their timing is random with clustering, not periodic.
4. **Transitions without zipper noise.** Smooth parameter interpolation per sample or per cycle, and no gain steps at the fuel-cut/resume switch unless the real recording shows a distinct "clunk".

**Diagnostic mapping** (symptom → first thing to measure):

| Symptom | First thing to measure |
|---|---|
| "Buzzy / mechanical" | Half-order levels (a); envelope modulation spectrum (c); per-cycle correlation (d) |
| "Too random / gravelly" | Jitter spectrum is white. Make it correlated and add deterministic per-cylinder structure |
| "Thin / synthetic" | Stochastic-to-harmonic ratio per ERB band (b); missing resonances |
| "Wrong on decel" | Torque-dependent timbre and rpm trajectory, not just pitch |
| "Clicky / steppy" | Boundary discontinuities; parameter smoothing |

### Gaps
- No academic measurement of order and broadband content during overrun/DFCO versus idle was found. Only enthusiast and tuning sources describe overrun acoustics.
- No measured typical half-order-to-firing-order level ratios (in dB) for healthy engines at idle were found. Measure them from the reference recordings, e.g. the Doerfler Zenodo dataset, DOI 10.5281/zenodo.16883336.
