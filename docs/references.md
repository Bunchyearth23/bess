# References for the standalone instrument

Only these sources were examined for this increment. No third-party source code, audio or measured parameters were copied.

| Source | Idea used | Limit and license |
| --- | --- | --- |
| [Julius O. Smith III, *Physical Audio Signal Processing*, delay-line interpolation](https://www.dsprelated.com/freebooks/pasp/Delay_Line_Signal_Interpolation.html) | Fractional delays can use linear interpolation; interpolation in feedback affects response. | BESS uses a deliberately lossy feedback loop and makes no claim of an exact waveguide. Technical reference, not copied code. |
| [DasEtwas, enginesound](https://github.com/DasEtwas/enginesound) and its [MIT license](https://github.com/DasEtwas/enginesound/blob/master/LICENSE) | Existing procedural engine tool demonstrates configuration and offline render workflows. | No code or audio reused. MIT license was checked only to assess possible future reuse. |
| [Ange Yaghi, engine-sim](https://github.com/ange-yaghi/engine-sim) and its [MIT license](https://github.com/ange-yaghi/engine-sim/blob/master/LICENSE) | Existing real-time engine sound project separates a simulation from synthesis/output. | No code or audio reused; its much fuller engine model is outside this prototype. MIT license was checked only to assess possible future reuse. |
| [Baldan et al., 2015, institutional record](https://air.iuav.it/handle/11578/264484) | Earlier BESS research used the high-level separation of ignition, intake and exhaust. | The full paper was not obtained in this increment; no detailed algorithm is attributed to it. See `ENGINE-SOUND-RESEARCH.md`. |

Farnell's *Designing Sound*, Jagla et al. (DOI 10.1121/1.4754663) and Munjal's *Acoustics of Ducts and Mufflers* were named in the mission but not read for this increment. No result or implementation detail is attributed to them here. The previous project research records abstract-level access limitations for Jagla and Baldan.
