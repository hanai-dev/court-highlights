// Test-only ONVIF camera: answers WS-Discovery probes on the test network and the three Media calls the box makes
// (GetCapabilities, GetProfiles, GetStreamUri), checking the WS-Security password digest like a real camera.
// Its stream is the fake court-3 camera on MediaMTX.
const dgram = require('node:dgram');
const http = require('node:http');
const crypto = require('node:crypto');

const USER = 'admin';
const PASSWORD = 'cam-pass-1';
const HOST = process.env.ONVIF_HOST || 'onvif-cam';
const STREAM = process.env.ONVIF_STREAM || 'rtsp://rtsp:8554/court3';

const udp = dgram.createSocket({ type: 'udp4', reuseAddr: true });
udp.on('message', (msg, rinfo) => {
  const text = msg.toString();
  if (!text.includes('Probe') || text.includes('ProbeMatches')) return;
  const relates = (text.match(/MessageID>([^<]+)</) || [])[1] || '';
  const reply = `<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery"><SOAP-ENV:Header><wsa:RelatesTo>${relates}</wsa:RelatesTo></SOAP-ENV:Header><SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><d:Types>dn:NetworkVideoTransmitter</d:Types><d:Scopes>onvif://www.onvif.org/name/Court%20Cam onvif://www.onvif.org/hardware/TestCam-4K</d:Scopes><d:XAddrs>http://${HOST}:8080/onvif/device_service</d:XAddrs></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>`;
  udp.send(reply, rinfo.port, rinfo.address);
});
udp.bind(3702, () => udp.addMembership('239.255.255.250'));

function authorized(body) {
  const user = (body.match(/<Username>([^<]*)</) || [])[1];
  const digest = (body.match(/PasswordDigest">([^<]*)</) || [])[1];
  const nonce = (body.match(/Base64Binary">([^<]*)</) || [])[1];
  const created = (body.match(/<Created[^>]*>([^<]*)</) || [])[1];
  if (!user || !digest || !nonce || !created) return false;
  const expected = crypto.createHash('sha1').update(Buffer.concat([Buffer.from(nonce, 'base64'), Buffer.from(created), Buffer.from(PASSWORD)])).digest('base64');
  return user === USER && digest === expected;
}

http
  .createServer((req, res) => {
    let body = '';
    req.on('data', (c) => (body += c));
    req.on('end', () => {
      if (!authorized(body)) {
        res.writeHead(401, { 'content-type': 'application/soap+xml' });
        return res.end('<s:Envelope><s:Body><s:Fault><s:Code><s:Subcode><s:Value>ter:NotAuthorized</s:Value></s:Subcode></s:Code></s:Fault></s:Body></s:Envelope>');
      }
      let answer;
      if (body.includes('GetCapabilities')) answer = `<tds:GetCapabilitiesResponse><tds:Capabilities><tt:Media><tt:XAddr>http://${HOST}:8080/onvif/media</tt:XAddr></tt:Media></tds:Capabilities></tds:GetCapabilitiesResponse>`;
      else if (body.includes('GetProfiles')) answer = '<trt:GetProfilesResponse><trt:Profiles token="main_profile" fixed="true"><tt:Name>Main</tt:Name></trt:Profiles></trt:GetProfilesResponse>';
      else if (body.includes('GetStreamUri')) answer = `<trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>${STREAM}</tt:Uri></trt:MediaUri></trt:GetStreamUriResponse>`;
      else { res.writeHead(400); return res.end(); }
      res.writeHead(200, { 'content-type': 'application/soap+xml' });
      res.end(`<?xml version="1.0"?><s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:tds="http://www.onvif.org/ver10/device/wsdl" xmlns:trt="http://www.onvif.org/ver10/media/wsdl" xmlns:tt="http://www.onvif.org/ver10/schema"><s:Body>${answer}</s:Body></s:Envelope>`);
    });
  })
  .listen(8080, () => console.log('fake ONVIF camera on :8080 and :3702'));
