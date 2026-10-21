// Test-only stand-in for the Amazon SES v2 API: accepts SendEmail calls (signature not checked) and keeps the
// messages in memory for the e2e suite. GET /messages lists them; DELETE /messages clears them.
// Like SES, once the account suppression list covers BOUNCE, mail to a hard-bounced address is accepted and dropped:
// POST /bounced {address} marks one; GET /suppression shows the account setting and what was dropped.
const http = require('node:http');

let messages = [];
let reasons = [];
const bounced = new Set();
const dropped = [];

const json = (res, status, value) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(value));
};

http
  .createServer((req, res) => {
    let body = '';
    req.on('data', (chunk) => (body += chunk));
    req.on('end', () => {
      if (req.method === 'POST' && req.url === '/v2/email/outbound-emails') {
        const m = JSON.parse(body);
        const simple = m.Content.Simple;
        const to = m.Destination.ToAddresses;
        if (reasons.includes('BOUNCE') && to.some((a) => bounced.has(a))) {
          dropped.push(...to);
          return json(res, 200, { MessageId: `suppressed-${dropped.length}` });
        }
        messages.push({ to, from: m.FromEmailAddress, subject: simple.Subject.Data, text: simple.Body.Text.Data, html: simple.Body.Html.Data, at: new Date().toISOString() });
        json(res, 200, { MessageId: `test-${messages.length}` });
      } else if (req.method === 'PUT' && req.url === '/v2/email/account/suppression') {
        reasons = JSON.parse(body).SuppressedReasons || [];
        json(res, 200, {});
      } else if (req.method === 'POST' && req.url === '/bounced') {
        bounced.add(JSON.parse(body).address);
        res.writeHead(204);
        res.end();
      } else if (req.method === 'GET' && req.url === '/suppression') {
        json(res, 200, { reasons, dropped });
      } else if (req.url === '/messages' && req.method === 'GET') {
        json(res, 200, messages);
      } else if (req.url === '/messages' && req.method === 'DELETE') {
        messages = [];
        res.writeHead(204);
        res.end();
      } else {
        res.writeHead(404);
        res.end();
      }
    });
  })
  .listen(8025, () => console.log('fake SES on :8025'));
