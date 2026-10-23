import { expect, test } from '@playwright/test';

test('shows devices and conflicts, and revokes a device', async ({ page }) => {
  let activeDevice = {
    device_id: '00000000-0000-7000-8000-000000000101',
    tenant_id: '00000000-0000-7000-8000-000000000001',
    store_id: '00000000-0000-7000-8000-000000000002',
    name: 'TPV Caja 1',
    status: 'active',
    enrolled_at: '2026-10-20T09:00:00Z',
    last_seen_at: '2026-10-23T12:00:00Z',
    revoked_at: null
  };
  const revokedDevice = {
    device_id: '00000000-0000-7000-8000-000000000102',
    tenant_id: '00000000-0000-7000-8000-000000000001',
    store_id: '00000000-0000-7000-8000-000000000002',
    name: 'TPV Caja 2',
    status: 'revoked',
    enrolled_at: '2026-10-18T09:00:00Z',
    last_seen_at: null,
    revoked_at: '2026-10-22T18:00:00Z'
  };
  const conflict = {
    id: '00000000-0000-7000-8000-000000000201',
    tenant_id: '00000000-0000-7000-8000-000000000001',
    store_id: '00000000-0000-7000-8000-000000000002',
    kind: 'product',
    entity_id: '00000000-0000-7000-8000-000000000301',
    local_version: 3,
    incoming: {
      kind: 'product',
      entity_id: '00000000-0000-7000-8000-000000000301',
      version: 3,
      device_id: '00000000-0000-7000-8000-000000000101',
      occurred_at: '2026-10-23T11:59:00Z',
      payload: { name: 'Camiseta básica', price: '12.00' },
      deleted: false
    },
    reason: 'same_version_diverged',
    outcome: 'accept_incoming',
    detected_at: '2026-10-23T12:00:00Z'
  };

  await page.route('**/health', (route) =>
    route.fulfill({ json: { status: 'ok', events: 3, cursor: 3 } })
  );
  await page.route('**/api/v1/events', (route) => route.fulfill({ json: [] }));
  await page.route('**/api/v1/devices', (route) =>
    route.fulfill({ json: [activeDevice, revokedDevice] })
  );
  await page.route('**/api/v1/master-data/conflicts', (route) =>
    route.fulfill({ json: [conflict] })
  );
  await page.route('**/api/v1/devices/*/revoke', async (route) => {
    expect(route.request().method()).toBe('POST');
    activeDevice = {
      ...activeDevice,
      status: 'revoked',
      revoked_at: '2026-10-23T12:05:00Z'
    };
    await route.fulfill({ json: activeDevice });
  });
  page.on('dialog', (dialog) => void dialog.accept());

  await page.goto('/');

  await expect(page.getByRole('heading', { name: 'Dispositivos registrados' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Conflictos detectados' })).toBeVisible();

  const deviceRow = page.getByRole('row', { name: /TPV Caja 1/ });
  await expect(deviceRow.getByText('Activo')).toBeVisible();
  await expect(deviceRow.getByRole('button', { name: 'Revocar' })).toBeEnabled();

  const conflictRow = page.getByRole('row', { name: /product/ });
  await expect(conflictRow.getByText('3 → 3')).toBeVisible();
  await expect(conflictRow.getByText('Entrante')).toBeVisible();
  await expect(conflictRow.getByText(/Camiseta básica/)).toBeVisible();

  await deviceRow.getByRole('button', { name: 'Revocar' }).click();

  await expect(deviceRow.getByText('Revocado')).toBeVisible();
  await expect(deviceRow.getByRole('button', { name: 'Revocar' })).toBeDisabled();
});
