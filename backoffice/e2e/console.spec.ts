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
  const saleEvent = {
    seq: 3,
    received_at: '2026-10-23T12:00:00Z',
    event: {
      event_id: '00000000-0000-7000-8000-000000000501',
      tenant_id: activeDevice.tenant_id,
      store_id: activeDevice.store_id,
      device_id: activeDevice.device_id,
      device_seq: 3,
      aggregate_type: 'sale',
      aggregate_id: '00000000-0000-7000-8000-000000000601',
      event_type: 'sale.confirmed',
      schema_version: 1,
      occurred_at: '2026-10-23T11:58:00Z',
      payload: {
        id: '00000000-0000-7000-8000-000000000601',
        total_cents: 1250,
        customer_id: '00000000-0000-7000-8000-000000000701',
        lines: [
          { description: 'Camiseta básica', quantity: '2', unit_price: '5.00', discount_bp: 0 },
          { description: 'Bolsa', quantity: '1', unit_price: '2.50', discount_bp: 0 }
        ],
        payments: [{ method: 'cash', amount: 1250 }]
      },
      hash: 'test-hash'
    }
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
  await page.route('**/api/v1/events*', (route) => {
    const eventType = new URL(route.request().url()).searchParams.get('event_type');
    const items = eventType && eventType !== 'sale.confirmed' ? [] : [saleEvent];
    return route.fulfill({
      json: { items, total: items.length, limit: 50, offset: 0 }
    });
  });
  await page.route('**/api/v1/sales*', (route) =>
    route.fulfill({ json: { items: [saleEvent], total: 1, limit: 50, offset: 0 } })
  );
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
  await page.route('**/api/v1/auth/status', (route) =>
    route.fulfill({ json: { has_users: true, suggested_tenant_id: null } })
  );
  await page.route('**/api/v1/auth/me', (route) =>
    route.fulfill({
      json: {
        id: '00000000-0000-7000-8000-000000000001',
        tenant_id: '00000000-0000-7000-8000-000000000001',
        username: 'central.admin',
        display_name: 'Administrador Central',
        role: 'admin'
      }
    })
  );
  await page.route('**/api/v1/auth/login', async (route) => {
    const body = route.request().postDataJSON() as { username?: string };
    const manager = body.username === 'manager';
    await route.fulfill({
      json: {
        token: manager ? 'manager-session-token' : 'test-session-token',
        expires_at: '2026-10-24T00:00:00Z',
        user: {
          id: '00000000-0000-7000-8000-000000000001',
          tenant_id: '00000000-0000-7000-8000-000000000001',
          username: manager ? 'manager' : 'central.admin',
          display_name: manager ? 'Responsable Central' : 'Administrador Central',
          role: manager ? 'manager' : 'admin'
        }
      }
    });
  });
  await page.route('**/api/v1/auth/logout', (route) => route.fulfill({ status: 204 }));
  page.on('dialog', (dialog) => {
    if (dialog.type() === 'prompt') {
      void dialog.accept('nuevaClave123');
    } else {
      void dialog.accept();
    }
  });

  await page.setViewportSize({ width: 1100, height: 700 });
  await page.goto('/');

  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');
  await page.getByRole('button', { name: 'Oscuro' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
  await page.getByRole('button', { name: 'Claro' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');

  await expect(page.getByRole('heading', { name: 'Acceso al backoffice' })).toBeVisible();
  await page.getByLabel('Usuario').fill('central.admin');
  await page.getByLabel('Contraseña').fill('central123');
  await page.getByRole('button', { name: 'Entrar' }).click();

  await expect(page.getByRole('heading', { name: 'Usuarios centrales' })).toBeVisible();
  const userFormOverflow = await page.evaluate(() => {
    const form = document.querySelector('.user-form');
    if (!form) return true;
    const button = form.querySelector('button');
    if (!button) return true;
    const bounds = form.getBoundingClientRect();
    const buttonRect = button.getBoundingClientRect();
    return [...form.querySelectorAll('input, select')].some((element) => {
      const rect = element.getBoundingClientRect();
      return (
        rect.left < bounds.left ||
        rect.right > bounds.right ||
        !(
          rect.right <= buttonRect.left ||
          rect.left >= buttonRect.right ||
          rect.bottom <= buttonRect.top ||
          rect.top >= buttonRect.bottom
        )
      );
    });
  });
  expect(userFormOverflow).toBe(false);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await expect(page.getByRole('heading', { name: 'Dispositivos registrados' })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Conflictos detectados' })).toBeVisible();
  await expect(page.getByRole('row', { name: /supervisor/ })).toBeVisible();

  await expect(page.getByRole('heading', { name: 'Ventas y trazabilidad' })).toBeVisible();
  const saleRow = page.getByRole('row', { name: /sale\.confirmed/ }).first();
  await expect(saleRow.getByText('12.50 €')).toBeVisible();
  await saleRow.getByRole('button', { name: 'Ver' }).click();
  const detailDialog = page.getByRole('dialog');
  await expect(detailDialog.getByRole('heading', { name: 'sale.confirmed' })).toBeVisible();
  await expect(
    detailDialog.getByRole('cell', { name: 'Camiseta básica', exact: true })
  ).toBeVisible();
  await expect(detailDialog.getByRole('cell', { name: 'cash', exact: true })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(detailDialog).toHaveCount(0);

  const downloadPromise = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Exportar página CSV' }).click();
  expect((await downloadPromise).suggestedFilename()).toMatch(/^ventas-\d{4}-\d{2}-\d{2}\.csv$/);

  await page.getByLabel('Tipo de evento').fill('invoice.issued');
  await page.getByRole('button', { name: 'Aplicar' }).click();
  await expect(page.getByText('Sin eventos.')).toBeVisible();
  await page.getByRole('button', { name: 'Limpiar' }).click();
  await expect(page.getByRole('row', { name: /sale\.confirmed/ }).first()).toBeVisible();

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

  await page.getByRole('button', { name: 'Salir' }).click();
  await expect(page.getByRole('heading', { name: 'Acceso al backoffice' })).toBeVisible();
  await page.getByLabel('Usuario').fill('manager');
  await page.getByLabel('Contraseña').fill('manager123');
  await page.getByRole('button', { name: 'Entrar' }).click();
  await expect(page.getByText(/Modo solo lectura/)).toBeVisible();
  await expect(page.getByRole('button', { name: 'Crear usuario' })).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Revocar' })).toHaveCount(0);
});
