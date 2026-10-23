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

  let centralUsers = [
    {
      id: '00000000-0000-7000-8000-000000000401',
      tenant_id: activeDevice.tenant_id,
      username: 'supervisor',
      display_name: 'Supervisora',
      role: 'manager',
      store_ids: [activeDevice.store_id],
      active: true,
      version: 1,
      created_at: '2026-10-19T09:00:00Z',
      updated_at: '2026-10-19T09:00:00Z'
    }
  ];

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
  await page.route('**/api/v1/users/*/password', async (route) => {
    expect(route.request().method()).toBe('POST');
    const userId = route.request().url().split('/').at(-2);
    const body = route.request().postDataJSON() as { password: string };
    expect(body.password).toBe('nuevaClave123');
    const user = centralUsers.find((item) => item.id === userId);
    if (!user) throw new Error('unknown user');
    user.version += 1;
    user.updated_at = '2026-10-23T12:10:00Z';
    await route.fulfill({ json: user });
  });
  await page.route('**/api/v1/users/*', async (route) => {
    expect(route.request().method()).toBe('PUT');
    const userId = route.request().url().split('/').at(-1);
    const body = route.request().postDataJSON() as {
      display_name: string;
      role: string;
      store_ids: string[];
      active: boolean;
    };
    const user = centralUsers.find((item) => item.id === userId);
    if (!user) throw new Error('unknown user');
    user.display_name = body.display_name;
    user.role = body.role;
    user.store_ids = body.store_ids;
    user.active = body.active;
    user.version += 1;
    user.updated_at = '2026-10-23T12:08:00Z';
    await route.fulfill({ json: user });
  });
  await page.route('**/api/v1/users', async (route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({ json: centralUsers });
      return;
    }
    expect(route.request().method()).toBe('POST');
    const body = route.request().postDataJSON() as {
      tenant_id: string;
      username: string;
      display_name: string;
      role: string;
      store_ids: string[];
      password: string;
    };
    expect(body.password).toBe('central123');
    const created = {
      id: crypto.randomUUID(),
      tenant_id: body.tenant_id,
      username: body.username,
      display_name: body.display_name,
      role: body.role,
      store_ids: body.store_ids,
      active: true,
      version: 1,
      created_at: '2026-10-23T12:00:00Z',
      updated_at: '2026-10-23T12:00:00Z'
    };
    centralUsers = [...centralUsers, created];
    await route.fulfill({ status: 201, json: created });
  });
  await page.route('**/api/v1/devices/*/revoke', async (route) => {
    expect(route.request().method()).toBe('POST');
    activeDevice = {
      ...activeDevice,
      status: 'revoked',
      revoked_at: '2026-10-23T12:05:00Z'
    };
    await route.fulfill({ json: activeDevice });
  });
  page.on('dialog', (dialog) => {
    if (dialog.type() === 'prompt') {
      void dialog.accept('nuevaClave123');
    } else {
      void dialog.accept();
    }
  });

  await page.goto('/');

  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');
  await page.getByRole('button', { name: 'Oscuro' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
  await page.getByRole('button', { name: 'Claro' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');

  await expect(page.getByRole('heading', { name: 'Usuarios centrales' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Dispositivos registrados' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Conflictos detectados' })).toBeVisible();
  await expect(page.getByRole('row', { name: /supervisor/ })).toBeVisible();

  await page.getByLabel('Usuario').fill('central.cashier');
  await page.getByLabel('Nombre visible').fill('Cajero Central');
  await page.getByLabel('Rol').selectOption('cashier');
  await page.getByLabel('Contraseña inicial').fill('central123');
  await page.getByRole('button', { name: 'Crear usuario' }).click();

  const userRow = page.getByRole('row', { name: /central.cashier/ });
  await expect(userRow).toBeVisible();
  await expect(userRow.getByText('Activo')).toBeVisible();
  await userRow.getByRole('button', { name: 'Desactivar' }).click();
  await expect(userRow.getByText('Inactivo')).toBeVisible();
  await userRow.getByRole('button', { name: 'Restablecer' }).click();
  await expect(userRow.getByText('3', { exact: true })).toBeVisible();

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
