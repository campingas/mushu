self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (ev) => ev.waitUntil(self.clients.claim()));

self.addEventListener('push', (ev) => {
  let data = { title: 'mushu', body: '', host: '', instance_url: '', pane_id: '', seq: 0 };
  try {
    data = ev.data.json();
  } catch (_) {}
  ev.waitUntil(
    self.registration.showNotification(data.title, {
      body: data.host ? `${data.host}: ${data.body}` : data.body,
      icon: '/icon-192.png',
      badge: '/icon-192.png',
      tag: `mushu-${data.host}-${data.pane_id || 'general'}`,
      data: {
        instance_url: data.instance_url || '',
        pane_id: data.pane_id || '',
        seq: Number(data.seq) || 0,
      },
    })
  );
});

self.addEventListener('notificationclick', (ev) => {
  ev.notification.close();
  const target = ev.notification.data || {};
  ev.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then(async (wins) => {
      for (const w of wins) {
        if ('focus' in w) {
          const focused = await w.focus();
          if (target.instance_url && target.pane_id) {
            focused.postMessage({ type: 'mushu-attention', target });
          }
          return focused;
        }
      }
      const query = new URLSearchParams();
      if (target.instance_url && target.pane_id) {
        query.set('mushu_instance', target.instance_url);
        query.set('mushu_pane', target.pane_id);
        query.set('mushu_seq', String(target.seq || 0));
      }
      const encoded = query.toString();
      return self.clients.openWindow(encoded ? `/?${encoded}` : '/');
    })
  );
});
