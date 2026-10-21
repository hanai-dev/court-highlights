// Full-text capture of the documentation pages the stack decision cites (rendered with Playwright).
// Usage: node stack/capture-docs.cjs   → stack/evidence/docs/<key>.txt
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

const OUT = path.join(__dirname, 'evidence', 'docs');
fs.mkdirSync(OUT, { recursive: true });
const PAGES = {
  'gst-licensing': 'https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html',
  'gst-va': 'https://gstreamer.freedesktop.org/documentation/va/index.html',
  'gst-releases': 'https://gstreamer.freedesktop.org/releases/',
  'ort-openvino-ep': 'https://onnxruntime.ai/docs/execution-providers/OpenVINO-ExecutionProvider.html',
  'ort-rs': 'https://ort.pyke.io/',
  'intel-n150': 'https://www.intel.com/content/www/us/en/products/sku/241636/intel-processor-n150-6m-cache-up-to-3-60-ghz/specifications.html',
  'jetson-orin': 'https://www.nvidia.com/en-us/autonomous-machines/embedded-systems/jetson-orin/',
  'jetpack': 'https://developer.nvidia.com/embedded/jetpack',
  'frigate-hardware': 'https://docs.frigate.video/frigate/hardware',
  'frigate-record': 'https://docs.frigate.video/configuration/record',
  'ffmpeg-legal': 'https://ffmpeg.org/legal.html',
  'r2-pricing': 'https://developers.cloudflare.com/r2/pricing/',
  'ses-pricing': 'https://aws.amazon.com/ses/pricing/',
  'do-droplets': 'https://www.digitalocean.com/pricing/droplets',
  'do-databases': 'https://www.digitalocean.com/pricing/managed-databases',
  'rust-stability': 'https://blog.rust-lang.org/2014/10/30/Stability.html',
  'ultralytics-license': 'https://www.ultralytics.com/license',
  'arxiv-2509.11132': 'https://arxiv.org/abs/2509.11132',
  'retina-readme': 'https://github.com/scottlamb/retina',
  'rauc': 'https://rauc.readthedocs.io/en/latest/basic.html',
  'jetson-modules': 'https://developer.nvidia.com/embedded/jetson-modules',
  'intel-media-driver': 'https://github.com/intel/media-driver',
  'frigate-dockerfile': 'https://github.com/blakeblackshear/frigate/blob/dev/docker/main/install_deps.sh',
  'jetson-store': 'https://www.nvidia.com/en-us/autonomous-machines/embedded-systems/jetson-orin/nano-super-developer-kit/',
  'seeed-orin-nx': 'https://www.seeedstudio.com/reComputer-J4012-p-5586.html',
  'beelink-eq14': 'https://www.bee-link.com/products/beelink-eq14-n150',
  'reolink-poe': 'https://reolink.com/product/rlc-510a/',
  'bsp-rates': 'https://www.bsp.gov.ph/SitePages/Statistics/ExchangeRate.aspx',
  'ubuntu-pro': 'https://ubuntu.com/pricing/pro',
  'systemd-sysupdate': 'https://www.freedesktop.org/software/systemd/man/latest/systemd-sysupdate.html',
};

(async () => {
  const only = process.argv.slice(2);
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36' });
  for (const [key, url] of Object.entries(PAGES)) {
    if (only.length && !only.includes(key)) continue;
    const page = await ctx.newPage();
    try {
      const res = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 45000 });
      await page.waitForTimeout(2500);
      const text = await page.evaluate(() => document.body.innerText);
      fs.writeFileSync(path.join(OUT, `${key}.txt`), `${url}\naccessed ${new Date().toISOString()}\nHTTP ${res ? res.status() : '?'}\n\n${text}\n`);
      console.log(`${key}: HTTP ${res ? res.status() : '?'} chars=${text.length}`);
    } catch (e) {
      console.log(`${key}: FAILED ${e.message.split('\n')[0]}`);
    }
    await page.close();
  }
  await browser.close();
})();
