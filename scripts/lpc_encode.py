#!/usr/bin/env python3
"""Encode a WAV recording as TMS5220 speech frames for the ICON.

Output is the text listing that the ICON's `encode_phrase` reads (and
`decode_phrase` prints): one 25 ms frame per line, as parameter indices

    energy pitch k1 k2 k3 k4 k5 k6 k7 k8 k9 k10

energy 0 is silence; pitch 0 is an unvoiced (noise) frame. On the ICON:
    encode_phrase <hello.txt >hello.sp ; say hello.sp

The analysis is the usual LPC-10 recipe (as in open-source TMS5220 encoders
such as python_wizard): pre-emphasis, Hamming-windowed autocorrelation,
Levinson-Durbin reflection coefficients, autocorrelation pitch detection,
then each parameter is quantised to the nearest entry of the TMS5220's own
tables. Needs numpy. Input: 8 kHz mono 16-bit WAV, e.g. from
    sox in.wav -r 8000 -c 1 -b 16 out.wav

usage: lpc_encode.py IN.wav > OUT.txt   [--gain G] [--pitch-shift S]
       lpc_encode.py IN.wav --paste NAME > NAME.paste.txt
The --paste form writes shell commands to paste into an ICON workstation (the
web page's "Type text" box): they write the listing to 1:/tmp/NAME.txt with
echo, encode it to 1:/tmp/NAME.sp and say it.
"""
import argparse
import sys
import wave

import numpy as np

# TMS5220 tables (indices -> values), from the chip / MAME tms5110r.hxx.
ENERGY = [0, 1, 2, 3, 4, 6, 8, 11, 16, 23, 33, 47, 63, 85, 114]  # 15 = stop
PITCH = [0, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29,
         30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 44, 46, 48,
         50, 52, 53, 56, 58, 60, 62, 65, 68, 70, 72, 76, 78, 80, 84, 86,
         91, 94, 98, 101, 105, 109, 114, 118, 122, 127, 132, 137, 142, 148, 153, 159]
K = [
    [-501, -498, -497, -495, -493, -491, -488, -482, -478, -474, -469, -464, -459, -452, -445, -437,
     -412, -380, -339, -288, -227, -158, -81, -1, 80, 157, 226, 287, 337, 379, 411, 436],
    [-328, -303, -274, -244, -211, -175, -138, -99, -59, -18, 24, 64, 105, 143, 180, 215,
     248, 278, 306, 331, 354, 374, 392, 408, 422, 435, 445, 455, 463, 470, 476, 506],
    [-441, -387, -333, -279, -225, -171, -117, -63, -9, 45, 98, 152, 206, 260, 314, 368],
    [-328, -273, -217, -161, -106, -50, 5, 61, 116, 172, 228, 283, 339, 394, 450, 506],
    [-328, -282, -235, -189, -142, -96, -50, -3, 43, 90, 136, 182, 229, 275, 322, 368],
    [-256, -212, -168, -123, -79, -35, 10, 54, 98, 143, 187, 232, 276, 320, 365, 409],
    [-308, -260, -212, -164, -117, -69, -21, 27, 75, 122, 170, 218, 266, 314, 361, 409],
    [-256, -161, -66, 29, 124, 219, 314, 409],
    [-256, -176, -96, -15, 65, 146, 226, 307],
    [-205, -132, -59, 14, 87, 160, 234, 307],
]
RATE = 8000
FRAME = 200  # 25 ms


def nearest(table, v):
    return int(np.argmin([abs(t - v) for t in table]))


def reflection_coeffs(x, order=10):
    """Levinson-Durbin on the autocorrelation of x; returns PARCOR k1..k10."""
    r = np.array([np.dot(x[: len(x) - i], x[i:]) for i in range(order + 1)])
    if r[0] <= 0:
        return np.zeros(order)
    r[0] *= 1.0001  # a little white-noise correction for stability
    a = np.zeros(order + 1)
    a[0] = 1.0
    err = r[0]
    k = np.zeros(order)
    for i in range(1, order + 1):
        acc = r[i] + np.dot(a[1:i], r[i - 1:0:-1])
        ki = -acc / err
        k[i - 1] = ki
        a[1:i] = a[1:i] + ki * a[i - 1:0:-1]
        a[i] = ki
        err *= 1 - ki * ki
        if err <= 0:
            break
    return k


def pitch_period(x):
    """Autocorrelation pitch detector; returns (period in samples, voicing strength)."""
    x = x - x.mean()
    # A crude low-pass (moving average) helps the detector.
    x = np.convolve(x, np.ones(4) / 4, mode="same")
    ac = np.correlate(x, x, mode="full")[len(x) - 1:]
    if ac[0] <= 0:
        return 0, 0.0
    lo, hi = RATE // 400, RATE // 60  # 60-400 Hz
    lag = lo + int(np.argmax(ac[lo:hi]))
    return lag, ac[lag] / ac[0]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("wav")
    ap.add_argument("--gain", type=float, default=0.5, help="scale the energy (default 0.5; louder clips the chip's DAC)")
    ap.add_argument("--pitch-shift", type=float, default=1.0, help="multiply pitch periods (>1 = lower voice)")
    ap.add_argument("--paste", metavar="NAME", help="write ICON shell commands that build and say 1:/tmp/NAME.sp")
    args = ap.parse_args()

    with wave.open(args.wav) as w:
        if w.getframerate() != RATE or w.getnchannels() != 1 or w.getsampwidth() != 2:
            sys.exit("need 8 kHz mono 16-bit WAV (sox in.wav -r 8000 -c 1 -b 16 out.wav)")
        s = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2").astype(float)

    pre = np.append(s[0], s[1:] - 0.9373 * s[:-1])  # pre-emphasis
    n = len(s) // FRAME
    window = np.hamming(2 * FRAME)
    rms = np.array([np.sqrt(np.mean(s[i * FRAME:(i + 1) * FRAME] ** 2)) for i in range(n)])
    peak = rms.max() or 1.0

    lines = []
    for i in range(n):
        # Analysis window: two frames centred on this one.
        a, b = max(0, i * FRAME - FRAME // 2), min(len(s), i * FRAME + FRAME + FRAME // 2)
        seg = pre[a:b]
        if len(seg) < FRAME:
            break
        e = min(14, nearest(ENERGY, min(114, 114 * args.gain * rms[i] / peak)))
        if rms[i] < peak * 0.02 or e == 0:
            lines.append([0] * 12)
            continue
        k = reflection_coeffs(seg * np.hamming(len(seg)))
        lag, strength = pitch_period(s[a:b])
        voiced = strength > 0.35
        p = nearest(PITCH[1:], lag * args.pitch_shift) + 1 if voiced else 0
        # The chip's k values are 512 * k in this Levinson-Durbin sign convention
        # (checked by rendering through the emulated chip: the other sign
        # garbles the spectrum).
        ks = [nearest(K[j], 512 * k[j]) for j in range(10)]
        if not voiced:
            ks[4:] = [0] * 6  # unvoiced frames carry only k1-k4
        lines.append([e, p] + ks)
    # Trim leading/trailing silence, end with one silent frame.
    while lines and lines[0][0] == 0:
        lines.pop(0)
    while lines and lines[-1][0] == 0:
        lines.pop()
    lines.append([0] * 12)
    text = [" ".join(str(v) for v in l) for l in lines]
    if not args.paste:
        print("\n".join(text))
        return
    t, sp = f"1:/tmp/{args.paste}.txt", f"1:/tmp/{args.paste}.sp"
    for i, l in enumerate(text):
        print(f"echo {l} {'>' if i == 0 else '>>'}{t}")
    print(f"encode_phrase <{t} >{sp}")
    print(f"say {sp}")


if __name__ == "__main__":
    main()
