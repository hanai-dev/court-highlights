// Test-only stand-in for the Amazon SES v2 API: accepts SendEmail calls (signature not checked) and keeps the
// messages in memory for the e2e suite. GET /messages lists them; DELETE /messages clears them.
const http = require('node:http');

let messages = [];

http
  .createServer((req, res) => {
    let body = '';
    req.on('data', (chunk) => (body += chunk));
    req.on('end', () => {
      if (req.method === 'POST' && req.url === '/v2/email/outbound-emails') {
        const m = JSON.parse(body);
        const simple = m.Content.Simple;
        messages.push({
          to: m.Destination.ToAddresses,
          from: m.FromEmailAddress,
          subject: simple.Subject.Data,
          text: simple.Body.Text.Data,
          html: simple.Body.Html.Data,
          at: new Date().toISOString(),
        });
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ MessageId: `test-${messages.length}` }));
      } else if (req.url === '/messages' && req.method === 'GET') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify(messages));
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
