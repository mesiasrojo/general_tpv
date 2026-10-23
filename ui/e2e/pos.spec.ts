import { expect, test } from '@playwright/test';

test.describe('TPV desktop UI', () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      const storeId = '00000000-0000-7000-8000-000000000001';
      const registerId = '00000000-0000-7000-8000-000000000002';
      const sellerId = '00000000-0000-7000-8000-000000000003';
      const customerId = '00000000-0000-7000-8000-000000000004';
      const userId = '00000000-0000-7000-8000-000000000005';
      const now = () => new Date().toISOString();

      let session: Record<string, unknown> | null = null;
      let sale: Record<string, unknown> | null = null;
      let totals = {
        gross: 0,
        net: 0,
        tax: 0,
        tax_breakdown: [],
        paid: 0,
        received: 0,
        due: 0,
        change: 0
      };

      const saleView = () => ({ sale, totals });
      const emptyTotals = () => ({
        gross: 0,
        net: 0,
        tax: 0,
        tax_breakdown: [],
        paid: 0,
        received: 0,
        due: 0,
        change: 0
      });

      const user = () => ({
        id: userId,
        username: 'admin',
        display_name: 'Administrador',
        role: 'admin',
        store_ids: [storeId],
        active: true
      });

      const api = {
        async current_user() {
          return session;
        },
        async login(username: string, password: string) {
          if (username !== 'admin' || password !== 'admin123') {
            throw 'credenciales inválidas';
          }
          session = { session_id: crypto.randomUUID(), user: user(), started_at: now() };
          return session;
        },
        async logout() {
          session = null;
        },
        async bootstrap() {
          return {
            store_name: '100%SHOPPING',
            store_id: storeId,
            register_id: registerId,
            seller_id: sellerId,
            customer_id: customerId,
            user_id: userId,
            products: [
              {
                id: '00000000-0000-7000-8000-000000000010',
                sku: '3',
                barcode: null,
                name: 'Café solo',
                family_code: null,
                unit_price: '1.20',
                tax_rate: { code: 'IVA10', percent_bp: 1000 },
                active: true
              }
            ],
            open_cash: null
          };
        },
        async list_sales() {
          return sale && sale.ticket_number ? [saleView()] : [];
        },
        async sync_status() {
          return {
            pending: 0,
            sent: 0,
            failed: 0,
            oldest_pending_at: null,
            last_sent_at: null,
            last_failed_at: null,
            last_error: null
          };
        },
        async list_failed_sync_events() {
          return [];
        },
        async search_products(query: string) {
          if (!query.toLowerCase().includes('caf')) {
            return [];
          }
          return [
            {
              sku: '3',
              name: 'Café solo',
              price_cents: 120,
              tax_code: 'IVA10',
              tax_percent_bp: 1000
            }
          ];
        },
        async start_sale() {
          sale = {
            id: crypto.randomUUID(),
            store_id: storeId,
            register_id: registerId,
            seller_id: sellerId,
            customer_id: customerId,
            lines: [],
            payments: [],
            state: 'draft',
            created_at: now(),
            closed_at: null,
            invoice_id: null,
            ticket_series: null,
            ticket_number: null,
            notes: null
          };
          totals = emptyTotals();
          return saleView();
        },
        async add_line(_saleId: string, sku: string) {
          if (!sale) throw 'venta no iniciada';
          sale.lines = [
            {
              id: crypto.randomUUID(),
              product_id: '00000000-0000-7000-8000-000000000010',
              sku,
              description: 'Café solo',
              unit_price: '1.20',
              quantity: '1',
              discount_bp: 0,
              tax_rate: { code: 'IVA10', percent_bp: 1000 },
              kind: 'sale'
            }
          ];
          sale.state = 'awaiting_payment';
          totals = {
            gross: 120,
            net: 109,
            tax: 11,
            tax_breakdown: [
              { code: 'IVA10', percent_bp: 1000, gross: 120, net: 109, tax: 11 }
            ],
            paid: 0,
            received: 0,
            due: 120,
            change: 0
          };
          return saleView();
        },
        async apply_payment(
          _saleId: string,
          method: string,
          amountCents: number,
          receivedCents: number
        ) {
          if (!sale) throw 'venta no iniciada';
          sale.payments = [
            {
              id: crypto.randomUUID(),
              method,
              amount: amountCents,
              received: receivedCents,
              reference: null
            }
          ];
          totals = {
            ...totals,
            paid: amountCents,
            received: receivedCents,
            due: 0,
            change: Math.max(0, receivedCents - amountCents)
          };
          return saleView();
        },
        async confirm_sale() {
          if (!sale) throw 'venta no iniciada';
          sale.state = 'paid';
          sale.ticket_number = 1;
          sale.ticket_series = 'TPV';
          sale.closed_at = now();
          return saleView();
        },
        async print_ticket() {
          return { job_id: crypto.randomUUID(), file: 'null', bytes: 1045, success: true };
        }
      };

      (window as any).__TAURI_INTERNALS__ = {
        invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
          switch (cmd) {
            case 'current_user':
              return api.current_user();
            case 'login':
              return api.login(String(args.username), String(args.password));
            case 'logout':
              return api.logout();
            case 'bootstrap':
              return api.bootstrap();
            case 'list_sales':
              return api.list_sales();
            case 'sync_status':
              return api.sync_status();
            case 'sync_now':
              return {
                accepted: 1,
                duplicates: 0,
                rejected: 0,
                pulled: 1,
                users_applied: 1,
                master_applied: 0,
                master_conflicts: 0
              };
            case 'list_failed_sync_events':
              return api.list_failed_sync_events();
            case 'search_products':
              return api.search_products(String(args.query));
            case 'start_sale':
              return api.start_sale();
            case 'add_line':
              return api.add_line(String(args.saleId), String(args.sku));
            case 'apply_payment':
              return api.apply_payment(
                String(args.saleId),
                String(args.method),
                Number(args.amountCents),
                Number(args.receivedCents)
              );
            case 'confirm_sale':
              return api.confirm_sale();
            case 'print_ticket':
              return api.print_ticket();
            default:
              throw new Error(`unexpected Tauri command: ${cmd}`);
          }
        },
        transformCallback: () => 1,
        unregisterCallback: () => undefined,
        convertFileSrc: (path: string) => path
      };
    });
  });

  test('login, add a product, take payment and print', async ({ page }) => {
    await page.goto('/');

    await expect(page.getByRole('heading', { name: 'Identifícate' })).toBeVisible();
    await page.getByLabel('Usuario').fill('admin');
    await page.getByLabel('Contraseña').fill('admin123');
    await page.getByRole('button', { name: 'Entrar' }).click();

    await expect(page.getByRole('heading', { name: '100%SHOPPING' })).toBeVisible();

    await page.getByRole('button', { name: 'Sincronizar ahora' }).click();
    await expect(page.getByText(/Sincronizado: 1 aceptados, 1 usuario/)).toBeVisible();

    const search = page.getByRole('textbox', { name: 'Buscar artículo' });
    await search.fill('café');
    await page.getByRole('button', { name: /3 · Café solo/ }).click();
    await expect(page.getByText(/Línea añadida/)).toBeVisible();

    await page.getByRole('button', { name: /F5 · Cobrar/ }).click();
    const payment = page.getByRole('dialog', { name: 'Forma de pago' });
    await expect(payment).toBeVisible();
    await payment.getByLabel('Entregado por el cliente (céntimos)').fill('200');
    await payment.getByRole('button', { name: 'Aplicar pago' }).click();
    await expect(page.getByText(/Cobro completo/)).toBeVisible();

    await payment.getByRole('button', { name: 'Cobrar y cerrar' }).click();
    await expect(page.getByText(/Ticket 1 confirmado e impreso/)).toBeVisible();
    await expect(page.getByText('Cobrada').first()).toBeVisible();
  });
});
