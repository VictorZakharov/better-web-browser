# Acid3 (http://acid3.acidtests.org/) as its server delivered it on 2026-10-25. The test states no
# licence, so the files are fetched at run time and checked against these hashes instead of being
# checked in. Status and Content-Type are part of the test: empty.css and support-b.png are
# deliberately served as text/html, and unknown paths return 404 with a PNG body.
@{
    Origin = 'http://acid3.acidtests.org/'
    # Served for every path that is not listed below.
    NotFound = @{ Status = 404; ContentType = 'image/png'; Body = 'support-a.png' }
    Files = @(
        @{ Path = '/'; File = 'index.html'; Status = 200; ContentType = 'text/html; charset=utf-8'; Sha256 = '54e921f770c775f1e255558c34c380792f102bbf0e81a3a64032afe3e1bc15b9' }
        @{ Path = '/empty.css'; Status = 200; ContentType = 'text/html; charset=utf-8'; Sha256 = 'e8e09d86f8d12e72b3365b4561a8581514fd75e314295849d81b280600a1e0e2' }
        @{ Path = '/empty.html'; Status = 200; ContentType = 'text/html; charset=utf-8'; Sha256 = '9b0b15ad9d21941a3222d75d13358492aea7d503e4e16e0db02929d3fe7d2dc8' }
        @{ Path = '/empty.png'; Status = 200; ContentType = 'image/png'; Sha256 = '4a49c92dcf775e8e6b97d58fdf3446f113cd618e648089ccd9f94c5bd7049b85' }
        @{ Path = '/empty.txt'; Status = 200; ContentType = 'text/plain; charset=utf-8'; Sha256 = 'f307e8cd9814a6b0a2cdf07b8267b17bfcaf1f7a6c8d12b7e79fdaf908885c8e' }
        @{ Path = '/empty.xml'; Status = 200; ContentType = 'application/xml; charset=utf-8'; Sha256 = '912318165cd5f199bb73298fb4258379eb31feb8fd20da9e7f1c70e50ddbbbf0' }
        @{ Path = '/font.ttf'; Status = 200; ContentType = 'application/x-truetype-font'; Sha256 = '63d2abd0041c3e3b4b52ad8d382353b53c51c6785e76ce56ed9dacad2d2e31c4' }
        @{ Path = '/svg.xml'; Status = 200; ContentType = 'image/svg+xml'; Sha256 = '53ee2497017992225df03338158d0a1b46d3455502c2f17555d6f7b24560a0a4' }
        @{ Path = '/support-a.png'; Status = 404; ContentType = 'image/png'; Sha256 = '743ea1d3aa160bcc2447e447bc2693b2bac4805c69c081f96defb9c5bde31edf' }
        @{ Path = '/support-b.png'; Status = 200; ContentType = 'text/html; charset=utf-8'; Sha256 = '21260b5e5d9ffd3158d14bf2c4c29a434505c9aa70babcc74d20cf3ea8a1c4b7' }
        @{ Path = '/support-c.png'; Status = 200; ContentType = 'image/png'; Sha256 = '743ea1d3aa160bcc2447e447bc2693b2bac4805c69c081f96defb9c5bde31edf' }
        @{ Path = '/xhtml.1'; Status = 200; ContentType = 'text/xml'; Sha256 = '7a352d803fece6f5493d434046e5e9218a1289b949106328f0f9a27d048e7bb1' }
        @{ Path = '/xhtml.2'; Status = 200; ContentType = 'text/xml'; Sha256 = 'bcc39dde9efd508db94f9a18cdb3d4a9766832b396b4e54f91b960c53e2028ac' }
        @{ Path = '/xhtml.3'; Status = 200; ContentType = 'text/xml'; Sha256 = '68d05f9bda8482c3e0dfb3e3cc12933dc0ad6b724862a792a03169725114bb8c' }
        @{ Path = '/reference.html'; Status = 200; ContentType = 'text/html; charset=utf-8'; Sha256 = '50df9eac88d116bb7f1c2d9f86ac1167dc0b92ae77713cf4620713e6b25e3876' }
        @{ Path = '/reference.png'; Status = 200; ContentType = 'image/png'; Sha256 = 'e39b3e928afc50b3386ad80c32e8de52654164bc3d2e18f95d3026439d5642f2' }
    )
}
