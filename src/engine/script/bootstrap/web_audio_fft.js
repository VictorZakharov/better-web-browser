    // Unscaled radix-2 transform shared by PeriodicWave synthesis and
    // AnalyserNode analysis. Callers apply their own Web Audio normalization.
    const audioFft = (real, imag, inverse) => {
        const size = real.length;
        for (let i = 1, j = 0; i < size; ++i) {
            let bit = size >> 1;
            while (j & bit) { j ^= bit; bit >>= 1; }
            j ^= bit;
            if (i < j) {
                [real[i], real[j]] = [real[j], real[i]];
                [imag[i], imag[j]] = [imag[j], imag[i]];
            }
        }
        for (let width = 2; width <= size; width *= 2) {
            const half = width / 2;
            const step = (inverse ? 2 : -2) * Math.PI / width;
            for (let offset = 0; offset < size; offset += width) {
                for (let k = 0; k < half; ++k) {
                    const cosine = Math.cos(step * k);
                    const sine = Math.sin(step * k);
                    const other = offset + k + half;
                    const turnReal = cosine * real[other] - sine * imag[other];
                    const turnImag = sine * real[other] + cosine * imag[other];
                    const first = offset + k;
                    real[other] = real[first] - turnReal;
                    imag[other] = imag[first] - turnImag;
                    real[first] += turnReal;
                    imag[first] += turnImag;
                }
            }
        }
    };
