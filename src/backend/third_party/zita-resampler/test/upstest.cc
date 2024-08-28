
#include <string.h>
#include <stdio.h>
#include <zita-resampler/resampler.h>
#include <zita-resampler/vresampler.h>


#define NCHAN 7
#define CHAN  3
#define HLEN  48
#define LINP  300
#define LOUT  1380  // LINP * 4.6


// Basic upsampling test of Resampler and Vresampler.
//
// We upsample by 4.6 and prefill the resamplers for zero delay.
// In the input signal we put a single sample with value 1 at
// offsets 0, 100 and 201. So in the output we expect a band-
// limited pulse (i.e. a windowed sinc() function) peaking at
// offsets 0, 460, and 924.6.
//
// Since we input exactly LINP + inpsize() - 1 samples, and the
// output buffer size is exactly the input size times the ratio,
// the input and output counters should both end up at 0.
//
// test1 > zz1
// gnuplot
// set grid
// plot 'zz1' u 1:2 w l lt1, 'zz1' u 1:3 w l lt 2
//
// Zoom in to verify the positions.


int main (int ac, char *av[])
{
    int         i;
    Resampler   R;
    VResampler  V;
    float       inp [NCHAN * LINP];
    float       out1 [NCHAN * LOUT];
    float       out2 [NCHAN * LOUT];
    float       *p1, *p2;

    // Clear input array.
    memset (inp, 0, NCHAN * LINP * sizeof (float));
    // Put a single sample at offsets 0 and 100.
    inp [CHAN] = 1;
    inp [CHAN + NCHAN * 100] = 1;
    inp [CHAN + NCHAN * 201] = 1;

    // Setup for upsampling by 46 / 10.
    R.setup (10, 46, NCHAN, HLEN);
    V.setup (4.6, NCHAN, HLEN);

    // Prefill for zero delay.
    R.out_count = LOUT;
    R.out_data = out1;
    R.inp_count = R.inpsize () / 2 - 1;
    R.inp_data = 0;
    R.process ();
    fprintf (stderr, "inp_count = %6d, out_count = %6d\n", R.inp_count, R.out_count);

    // Process the entire input buffer.
    R.inp_count = LINP;
    R.inp_data = inp;
    R.process ();
    fprintf (stderr, "inp_count = %6d, out_count = %6d\n", R.inp_count, R.out_count);

    // Postfill to complete.
    R.inp_count = R.inpsize () / 2;
    R.inp_data = 0;
    R.process ();
    fprintf (stderr, "inp_count = %6d, out_count = %6d\n", R.inp_count, R.out_count);

    // Same for VResampler
    V.out_count = LOUT;
    V.out_data = out2;
    V.inp_count = R.inpsize () / 2 - 1;
    V.inp_data = 0;
    V.process ();

    V.inp_count = LINP;
    V.inp_data = inp;
    V.process ();

    V.inp_count = R.inpsize () / 2;
    V.inp_data = 0;
    V.process ();

    // Write both the Resampler and VResampler outputs, they
    // should be identical.
    p1 = out1;
    p2 = out2;
    for (i = 0; i < LOUT; i++)
    {
        printf ("%5d %10.8lf %10.8lf\n", i, p1 [CHAN], p2 [CHAN]);
	p1 += NCHAN;
	p2 += NCHAN;
    }
    return 0;
}
