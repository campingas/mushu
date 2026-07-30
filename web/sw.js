self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', (ev) => ev.waitUntil(self.clients.claim()));

self.addEventListener('push', (ev) => {
  let data = { title: 'mushu', body: '', host: '' };
  try {
    data = ev.data.json();
  } catch (_) {}
  ev.waitUntil(
    self.registration.showNotification(data.title, {
      body: data.host ? `${data.host}: ${data.body}` : data.body,
      icon: '/icon-192.png',
      badge: '/icon-192.png',
      tag: `mushu-${data.host}`,
    })
  );
});

self.addEventListener('notificationclick', (ev) => {
  ev.notification.close();
  ev.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((wins) => {
      for (const w of wins) {
        if ('focus' in w) return w.focus();
      }
      return self.clients.openWindow('/');
    })
  );
});
