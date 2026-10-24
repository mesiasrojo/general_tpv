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

      const customerRecords: any[] = [
        {
          id: customerId,
          first_name: 'Cliente',
          last_name1: 'Contado',
          last_name2: null,
          phone1: null,
          email: null,
          nif: '12345678Z',
          address: { street: '', number: '', locality: '', postal_code: '', province: '' },
          active: true
        }
      ];

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

      const catalogItems: Array<Record<string, unknown>> = [];
      const sellerItems: Array<Record<string, unknown>> = [];
      const storeItems: Array<Record<string, unknown>> = [
        {
          id: storeId,
          code: 'BEN01',
          name: 'Caja 1',
          tax_id: 'B12345674',
          phone: null,
          address: { street: '', number: '', locality: '', postal_code: '', province: '' },
          active: true
        }
      ];
      const familyItems: Array<Record<string, unknown>> = [
        {
          id: '00000000-0000-7000-8000-000000000010',
          code: 'GENERAL',
          name: 'General',
          active: true
        }
      ];
      const taxItems: Array<Record<string, unknown>> = [
        {
          id: '00000000-0000-7000-8000-000000000011',
          code: 'IVA21',
          name: 'IVA general 21%',
          percent_bp: 2100,
          active: true
        }
      ];
      let cashSession: any = null;
      const timeEntries: any[] = [];
      const timeClockStatus = () => {
        const active = timeEntries.find((entry) => !entry.clock_out) ?? null;
        return {
          active: active ? { entry: active, user_name: 'Administrador' } : null,
          recent: [...timeEntries]
            .reverse()
            .map((entry) => ({ entry, user_name: 'Administrador' }))
        };
      };
      const noteViews: any[] = [];
      const noteInbox = () => ({
        notes: [...noteViews].reverse(),
        unread: noteViews.filter((view) => !view.is_author && !view.viewer_read_at).length,
        recipients: [{ user_id: userId, user_name: 'Administrador' }]
      });
      let invoiceRecord: any = null;
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
        async open_cash() {
          cashSession = {
            id: crypto.randomUUID(),
            store_id: storeId,
            register_id: registerId,
            opened_by: userId,
            opened_at: now(),
            opening_float: 10000,
            movements: [],
            close_history: [],
            reopen_history: [],
            state: 'open',
            version: 1
          };
          return cashSession;
        },
        async cash_status() {
          return cashSession;
        },
        async cash_movement(
          sessionId: string,
          kind: string,
          amountCents: number,
          reason: string
        ) {
          if (!cashSession || cashSession.id !== sessionId) throw 'caja no abierta';
          cashSession.movements.push({
            id: crypto.randomUUID(),
            kind,
            amount: amountCents,
            reason,
            created_at: now()
          });
          return cashSession;
        },
        async time_clock_status() {
          return timeClockStatus();
        },
        async time_clock_in() {
          if (timeEntries.some((entry) => !entry.clock_out)) {
            throw 'Ya existe un fichaje abierto.';
          }
          timeEntries.push({
            id: crypto.randomUUID(),
            user_id: userId,
            store_id: storeId,
            clock_in: now(),
            clock_out: null,
            correction_reason: null
          });
          return timeClockStatus();
        },
        async time_clock_out() {
          const active = timeEntries.find((entry) => !entry.clock_out);
          if (!active) throw 'No hay ningún fichaje abierto.';
          active.clock_out = now();
          return timeClockStatus();
        },
        async note_list() {
          return noteInbox();
        },
        async note_create(request: any) {
          const note = {
            id: crypto.randomUUID(),
            store_id: storeId,
            author_id: userId,
            title: request.title,
            body: request.body,
            created_at: now()
          };
          noteViews.push({
            note,
            author_name: 'Administrador',
            recipients: (request.recipient_ids as string[]).map((id) => ({
              user_id: id,
              user_name: 'Administrador',
              read_at: null
            })),
            viewer_read_at: null,
            is_author: true
          });
          return noteInbox();
        },
        async note_read(noteId: string) {
          const view = noteViews.find((item) => item.note.id === noteId);
          if (!view) throw 'Nota no encontrada.';
          view.viewer_read_at = now();
          return noteInbox();
        },
        async list_sales() {
          return sale && sale.ticket_number ? [saleView()] : [];
        },
        async query_sales(request: any) {
          const hasSale = Boolean(sale && sale.ticket_number);
          const ticket = request.ticket_number;
          const matches =
            hasSale && (ticket == null || Number(ticket) === Number(sale?.ticket_number));
          const items = matches ? [saleView()] : [];
          return {
            items,
            total: items.length,
            limit: request.limit ?? 25,
            offset: request.offset ?? 0,
            gross: items.length ? totals.gross : 0,
            tax: items.length ? totals.tax : 0,
            paid: items.length ? totals.paid : 0
          };
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
        async list_customers() {
          return customerRecords;
        },
        async list_users() {
          return [user()];
        },
        async list_stores() {
          return storeItems;
        },
        async save_customer(request: any) {
          const existing = request.customer_id
            ? customerRecords.find((customer) => customer.id === request.customer_id)
            : null;
          const record = {
            id: existing?.id ?? crypto.randomUUID(),
            first_name: request.first_name,
            last_name1: request.last_name1,
            last_name2: request.last_name2 ?? null,
            phone1: request.phone ?? null,
            email: request.email ?? null,
            nif: request.nif ?? null,
            address: existing?.address ?? {
              street: '',
              number: '',
              locality: '',
              postal_code: '',
              province: ''
            },
            active: request.active
          };
          if (existing) {
            Object.assign(existing, record);
          } else {
            customerRecords.push(record);
          }
          return record;
        },
        async start_sale(customerIdArg?: string) {
          sale = {
            id: crypto.randomUUID(),
            store_id: storeId,
            register_id: registerId,
            seller_id: sellerId,
            customer_id: customerIdArg ?? customerId,
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
        async issue_invoice() {
          if (!sale) throw 'venta no iniciada';
          sale.invoice_id = '00000000-0000-7000-8000-000000000901';
          const source = sale.lines[0];
          invoiceRecord = {
            id: sale.invoice_id,
            sale_id: sale.id,
            customer_id: sale.customer_id ?? customerId,
            series: 'FAC',
            number: 1,
            issued_at: now(),
            kind: { type: 'standard' },
            lines: [
              {
                source_line_id: source.id,
                description: source.description,
                quantity: source.quantity,
                unit_price: source.unit_price,
                discount_bp: 0,
                tax_rate: source.tax_rate,
                gross: totals.gross,
                tax: totals.tax,
                kind: 'sale'
              }
            ],
            totals
          };
          return invoiceRecord;
        },
        async get_invoice(invoiceId: string) {
          if (!invoiceRecord || invoiceRecord.id !== invoiceId) throw 'factura no encontrada';
          return invoiceRecord;
        },
        async rectify_invoice(invoiceId: string, reason: string) {
          return {
            id: crypto.randomUUID(),
            sale_id: sale?.id ?? '',
            customer_id: sale?.customer_id ?? customerId,
            series: 'REC',
            number: 1,
            issued_at: now(),
            kind: {
              type: 'rectificative',
              data: { original_invoice_id: invoiceId, reason }
            },
            lines: [],
            totals
          };
        },
        async refund_invoice_lines(
          invoiceId: string,
          reason: string,
          refundMethod: string,
          _refundReference: string | null,
          lines: Array<{ source_line_id: string; quantity: string }>
        ) {
          if (!invoiceRecord || invoiceRecord.id !== invoiceId) throw 'factura no encontrada';
          let amount = 0;
          for (const selection of lines) {
            const line = invoiceRecord.lines.find(
              (entry: any) => entry.source_line_id === selection.source_line_id
            );
            if (!line) throw 'línea no encontrada';
            amount += Math.round(
              (Math.abs(line.gross) * Number(selection.quantity)) / Number(line.quantity)
            );
          }
          if (refundMethod === 'cash' && cashSession) {
            cashSession.movements.push({
              id: crypto.randomUUID(),
              kind: 'expense',
              amount,
              reason: 'Devolución REC-000001',
              created_at: now()
            });
          }
          return {
            invoice: {
              id: crypto.randomUUID(),
              sale_id: invoiceRecord.sale_id,
              customer_id: invoiceRecord.customer_id,
              series: 'REC',
              number: 1,
              issued_at: now(),
              kind: {
                type: 'rectificative',
                data: { original_invoice_id: invoiceId, reason }
              },
              lines: [],
              totals: {
                ...totals,
                gross: -amount,
                net: -amount,
                tax: 0,
                tax_breakdown: [],
                paid: 0,
                received: 0,
                due: 0,
                change: 0
              }
            },
            cash: refundMethod === 'cash' ? cashSession : null
          };
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
            case 'list_customers':
              return api.list_customers();
            case 'list_users':
              return api.list_users();
            case 'list_stores':
              return api.list_stores();
            case 'save_customer':
              return api.save_customer(args.request);
            case 'open_cash':
              return api.open_cash();
            case 'cash_status':
              return api.cash_status();
            case 'cash_movement':
              return api.cash_movement(
                String(args.sessionId),
                String(args.kind),
                Number(args.amountCents),
                String(args.reason)
              );
            case 'list_sales':
              return api.list_sales();
            case 'query_sales':
              return api.query_sales(args.request);
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
            case 'time_clock_status':
              return api.time_clock_status();
            case 'time_clock_in':
              return api.time_clock_in();
            case 'time_clock_out':
              return api.time_clock_out();
            case 'note_list':
              return api.note_list();
            case 'note_create':
              return api.note_create(args.request);
            case 'note_read':
              return api.note_read(String(args.noteId));
            case 'search_products':
              return api.search_products(String(args.query));
            case 'list_products':
              return catalogItems;
            case 'list_families':
              return familyItems;
            case 'list_taxes':
              return taxItems;
            case 'list_sellers':
              return sellerItems;
            case 'list_master_data_conflicts':
              return [];
            case 'save_product': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.product_id ?? crypto.randomUUID(),
                sku: request.sku,
                barcode: request.barcode ?? null,
                name: request.name,
                family_code: request.family_code ?? null,
                unit_price: request.unit_price,
                tax_rate: {
                  code: request.tax_code,
                  percent_bp: request.tax_percent_bp
                },
                active: request.active
              };
              const index = catalogItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                catalogItems[index] = saved;
              } else {
                catalogItems.push(saved);
              }
              return saved;
            }
            case 'save_family': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.family_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                active: request.active
              };
              const index = familyItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                familyItems[index] = saved;
              } else {
                familyItems.push(saved);
              }
              return saved;
            }
            case 'save_tax': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.tax_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                percent_bp: request.percent_bp,
                active: request.active
              };
              const index = taxItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                taxItems[index] = saved;
              } else {
                taxItems.push(saved);
              }
              return saved;
            }
            case 'save_seller': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.seller_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                phone1: request.phone1 ?? null,
                phone2: request.phone2 ?? null,
                active: request.active
              };
              const index = sellerItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                sellerItems[index] = saved;
              } else {
                sellerItems.push(saved);
              }
              return saved;
            }
            case 'save_store': {
              const request = args.request as Record<string, unknown>;
              const saved = {
                id: request.store_id ?? crypto.randomUUID(),
                code: String(request.code).toUpperCase(),
                name: request.name,
                tax_id: String(request.tax_id).toUpperCase(),
                phone: request.phone ?? null,
                address: {
                  street: request.street ?? '',
                  number: request.number ?? '',
                  locality: request.locality ?? '',
                  postal_code: request.postal_code ?? '',
                  province: request.province ?? ''
                },
                active: request.active
              };
              const index = storeItems.findIndex((item) => item.id === saved.id);
              if (index >= 0) {
                storeItems[index] = saved;
              } else {
                storeItems.push(saved);
              }
              return saved;
            }
            case 'preview_product_import':
              return {
                source_system: String(args.sourceSystem),
                batch_id: 'e2e-batch',
                dry_run: true,
                committed: false,
                already_imported: false,
                total_rows: 2,
                valid_rows: 2,
                invalid_rows: 0,
                inserted: 1,
                updated: 1,
                unchanged: 0,
                errors: []
              };
            case 'commit_product_import':
              return {
                source_system: String(args.sourceSystem),
                batch_id: 'e2e-batch',
                dry_run: false,
                committed: true,
                already_imported: false,
                total_rows: 2,
                valid_rows: 2,
                invalid_rows: 0,
                inserted: 1,
                updated: 1,
                unchanged: 0,
                errors: []
              };
            case 'start_sale':
              return api.start_sale(args.customerId ? String(args.customerId) : undefined);
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
            case 'issue_invoice':
              return api.issue_invoice();
            case 'rectify_invoice':
              return api.rectify_invoice(String(args.invoiceId), String(args.reason));
            case 'get_invoice':
              return api.get_invoice(String(args.invoiceId));
            case 'refund_invoice_lines':
              return api.refund_invoice_lines(
                String(args.invoiceId),
                String(args.reason),
                String(args.refundMethod),
                args.refundReference ? String(args.refundReference) : null,
                args.lines as Array<{ source_line_id: string; quantity: string }>
              );
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
    page.on('dialog', (dialog) => void dialog.accept('Devolución E2E'));
    await page.setViewportSize({ width: 1100, height: 700 });
    await page.goto('/');

    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');
    await page.getByRole('button', { name: 'Oscuro' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark_theme');
    await page.getByRole('button', { name: 'Claro' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light_theme');

    await expect(page.getByRole('heading', { name: 'Identifícate' })).toBeVisible();
    await page.getByLabel('Usuario').fill('admin');
    await page.getByLabel('Contraseña').fill('admin123');
    await page.getByRole('button', { name: 'Entrar' }).click();

    await expect(page.getByRole('heading', { name: '100%SHOPPING' })).toBeVisible();

    await page.getByRole('button', { name: 'Sincronizar ahora' }).click();
    await expect(page.getByText(/Sincronizado: 1 aceptados, 1 usuario/)).toBeVisible();

    await page.getByRole('button', { name: 'Fichar', exact: true }).click();
    const timeClockDialog = page.getByRole('dialog', { name: 'Fichaje del personal' });
    await expect(timeClockDialog).toBeVisible();
    await expect(timeClockDialog.getByText('No tienes ningún fichaje abierto.')).toBeVisible();
    await timeClockDialog.getByRole('button', { name: 'Fichar entrada' }).click();
    await expect(timeClockDialog.getByRole('button', { name: 'Fichar salida' })).toBeVisible();
    await expect(timeClockDialog.getByText('En curso')).toBeVisible();
    await timeClockDialog.getByRole('button', { name: 'Fichar salida' }).click();
    await expect(timeClockDialog.getByRole('button', { name: 'Fichar entrada' })).toBeVisible();
    await expect(timeClockDialog.locator('.timeclock-row')).toHaveCount(1);
    await expect(timeClockDialog.locator('.timeclock-row')).toContainText('Administrador');
    await timeClockDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByRole('button', { name: 'Notas', exact: true }).click();
    const notesDialog = page.getByRole('dialog', { name: 'Notas internas' });
    await expect(notesDialog).toBeVisible();
    await expect(notesDialog.getByText('No hay notas en la bandeja.')).toBeVisible();
    await notesDialog.getByLabel('Título').fill('Aviso E2E');
    await notesDialog.getByLabel('Contenido').fill('Revisar la caja antes de cerrar.');
    await notesDialog.getByRole('checkbox', { name: 'Administrador' }).check();
    await notesDialog.getByRole('button', { name: 'Publicar nota' }).click();
    await expect(notesDialog.locator('.note-card')).toHaveCount(1);
    await expect(notesDialog.locator('.note-card')).toContainText('Aviso E2E');
    await expect(notesDialog.locator('.note-card')).toContainText('Para: Administrador');
    await notesDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByRole('button', { name: 'Clientes' }).click();
    const customers = page.getByRole('dialog', { name: 'Clientes' });
    await expect(customers).toBeVisible();
    await customers.getByLabel('Nombre').fill('Cliente');
    await customers.getByLabel('Primer apellido').fill('UI');
    await customers.getByLabel('NIF').fill('12345678Z');
    await customers.getByRole('button', { name: 'Guardar cliente' }).click();
    await expect(customers.locator('.customer-row', { hasText: 'Cliente UI' })).toBeVisible();
    await customers.getByRole('button', { name: 'Esc' }).click();
    await page.getByLabel('Cliente').selectOption({ label: 'Cliente UI · 12345678Z' });

    await page.getByRole('button', { name: 'Abrir caja · fondo 100,00 €' }).click();
    await expect(page.getByText('Caja abierta con fondo de 100,00 €.')).toBeVisible();
    await page.getByLabel('Importe del movimiento (€)').fill('3,50');
    await page.getByLabel('Motivo del movimiento').fill('Compra de hielo');
    await page.getByRole('button', { name: 'Registrar movimiento' }).click();
    await expect(page.getByText(/Movimiento registrado: Gasto 3.50 €/)).toBeVisible();
    const movementRow = page.locator('.movement-row', { hasText: 'Compra de hielo' });
    await expect(movementRow).toContainText('Gasto');
    await expect(movementRow).toContainText('−3.50 €');

    await page.getByRole('button', { name: 'Catálogo' }).click();
    await expect(page.getByRole('dialog', { name: 'Catálogo' })).toBeVisible();
    await expect(page.getByText('Sin conflictos de maestros.')).toBeVisible();

    await page.getByRole('tab', { name: 'Familias' }).click();
    await page.getByLabel('Código').fill('E2E');
    await page.getByLabel('Nombre').fill('Familia E2E');
    await page.getByRole('button', { name: 'Guardar familia' }).click();
    await expect(page.locator('.product-row', { hasText: 'Familia E2E' })).toBeVisible();

    await page.getByRole('tab', { name: 'Impuestos' }).click();
    await page.getByLabel('Código').fill('E2E5');
    await page.getByLabel('Nombre').fill('Especial E2E');
    await page.getByLabel('% IVA').fill('5');
    await page.getByRole('button', { name: 'Guardar impuesto' }).click();
    await expect(page.locator('.product-row', { hasText: 'Especial E2E' })).toContainText('5% IVA');

    await page.getByRole('tab', { name: 'Vendedores' }).click();
    await page.getByLabel('Código').fill('E2E-S');
    await page.getByLabel('Nombre').fill('Vendedor E2E');
    await page.getByLabel('Teléfono 1').fill('600111222');
    await page.getByRole('button', { name: 'Guardar vendedor' }).click();
    await expect(page.locator('.product-row', { hasText: 'Vendedor E2E' })).toContainText('600111222');

    await page.getByRole('tab', { name: 'Tiendas' }).click();
    await page.getByLabel('Código', { exact: true }).fill('E2E-STORE');
    await page.getByLabel('Nombre').fill('Tienda E2E');
    await page.getByLabel('NIF').fill('B12345674');
    await page.getByLabel('Calle').fill('Calle E2E');
    await page.getByLabel('Número').fill('7');
    await page.getByLabel('Localidad').fill('Benalmádena');
    await page.getByLabel('Código postal').fill('29630');
    await page.getByLabel('Provincia').fill('Málaga');
    await page.getByRole('button', { name: 'Guardar tienda' }).click();
    await expect(page.locator('.product-row', { hasText: 'Tienda E2E' })).toContainText('B12345674');

    await page.getByRole('tab', { name: 'Artículos' }).click();
    await page.getByLabel('SKU').fill('UI-E2E-1');
    await page.getByLabel('Nombre').fill('Producto E2E');
    await page.getByLabel('Familia').selectOption('E2E');
    await page.getByLabel('Código fiscal').selectOption('E2E5');
    await page.getByLabel('Precio').fill('7.50');
    await page.getByRole('button', { name: 'Guardar producto' }).click();
    const productRow = page.locator('.product-row', { hasText: 'Producto E2E' });
    await expect(productRow).toBeVisible();
    await expect(productRow).toContainText('5% IVA');
    await productRow.getByRole('button', { name: 'Editar' }).click();
    await page.getByLabel('Nombre').fill('Producto E2E editado');
    await page.getByRole('button', { name: 'Guardar producto' }).click();
    await expect(page.locator('.product-row', { hasText: 'Producto E2E editado' })).toBeVisible();

    await page.getByLabel('Fichero CSV').setInputFiles({
      name: 'productos.csv',
      mimeType: 'text/csv',
      buffer: Buffer.from(
        'source_id,sku,name,unit_price,tax_percent_bp\nlegacy-1,IMP-1,Importado,2.00,2100\n'
      )
    });
    await expect(page.getByText('Lote e2e-batch')).toBeVisible();
    await expect(page.getByText('Simulación: todavía no se ha escrito nada.')).toBeVisible();
    await page.getByRole('button', { name: 'Confirmar importación' }).click();
    await expect(page.getByText('IMPORTACIÓN CONFIRMADA', { exact: true })).toBeVisible();
    await expect(
      page.getByText('Importación confirmada: 1 nuevos, 1 actualizados.')
    ).toBeVisible();
    await page.getByRole('dialog', { name: 'Catálogo' }).getByRole('button', { name: 'Esc' }).click();
    await page.getByRole('button', { name: 'Usuarios' }).click();
    const usersDialog = page.getByRole('dialog', { name: 'Gestión de usuarios' });
    await expect(usersDialog).toBeVisible();
    await expect(usersDialog.getByText('admin', { exact: true })).toBeVisible();
    const userFormOverflow = await usersDialog.evaluate((dialog) => {
      const form = dialog.querySelector('.user-create');
      if (!form) return true;
      const button = form.querySelector('button');
      if (!button) return true;
      const bounds = dialog.getBoundingClientRect();
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
    await usersDialog.getByRole('button', { name: 'Esc' }).click();

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

    await page.getByRole('button', { name: 'Consultas', exact: true }).click();
    const historyDialog = page.getByRole('dialog', { name: 'Consultas de ventas' });
    await expect(historyDialog).toBeVisible();
    await expect(historyDialog.locator('.history-row')).toHaveCount(1);
    await historyDialog.locator('.history-row').click();
    await expect(historyDialog.getByRole('heading', { name: /Ticket #1/ })).toBeVisible();
    await expect(historyDialog.getByText('Café solo')).toBeVisible();
    await historyDialog.getByRole('button', { name: 'Volver' }).click();
    await historyDialog.getByLabel('Ticket').fill('1');
    await historyDialog.getByRole('button', { name: 'Buscar' }).click();
    await expect(historyDialog.locator('.history-row')).toHaveCount(1);
    await historyDialog.getByLabel('Ticket').fill('99');
    await historyDialog.getByRole('button', { name: 'Buscar' }).click();
    await expect(historyDialog.locator('.history-row')).toHaveCount(0);
    await expect(historyDialog.getByText('No hay ventas para esos filtros.')).toBeVisible();
    await historyDialog.getByRole('button', { name: 'Esc' }).click();

    await page.getByRole('button', { name: 'Emitir factura' }).click();
    await expect(page.getByText(/Factura FAC-000001 emitida/)).toBeVisible();

    await page.getByRole('button', { name: 'Devolver' }).first().click();
    const refund = page.getByRole('dialog', { name: 'Devolución parcial' });
    await expect(refund).toBeVisible();
    await expect(refund.getByLabel('Referencia del reembolso')).toBeVisible();
    await refund.getByLabel('Cantidad a devolver Café solo').fill('0,5');
    await refund.getByLabel('Motivo de la devolución').fill('Devolución parcial E2E');
    await refund.getByRole('button', { name: 'Emitir devolución' }).click();
    await expect(page.getByText(/Devolución REC-000001 emitida/)).toBeVisible();
    await expect(page.locator('.movement-row', { hasText: 'Devolución REC-000001' })).toBeVisible();

    await page.getByRole('button', { name: 'Rectificar' }).first().click();
    await expect(page.getByText(/Rectificación REC-000001 emitida/)).toBeVisible();
  });
});
