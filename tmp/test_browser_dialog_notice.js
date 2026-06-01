const puppeteer = require('puppeteer-core');

(async () => {
    const browser = await puppeteer.launch({
        executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        headless: 'new'
    });

    try {
        const page = await browser.newPage();
        const nativeDialogs = [];

        page.on('dialog', async dialog => {
            nativeDialogs.push(dialog.message());
            await dialog.dismiss();
        });

        await page.goto('http://127.0.0.1:8080/login');
        await page.type('input[name="username"]', 'admin');
        await page.type('input[name="password"]', 'admin');
        await Promise.all([
            page.waitForNavigation(),
            page.click('button[type="submit"]')
        ]);

        await page.goto('http://127.0.0.1:8080/jobs');

        const state = await page.evaluate(() => ({
            hasDialogRoot: !!document.getElementById('app-dialog-root'),
            hasToastRegion: !!document.getElementById('app-toast-region'),
            hasConfirmFn: typeof window.appConfirm,
            hasToastFn: typeof window.appToast,
            hasInlineFn: typeof window.renderInlineNotice
        }));

        if (!state.hasDialogRoot || !state.hasToastRegion) {
            throw new Error(JSON.stringify(state));
        }

        if (state.hasConfirmFn !== 'function' || state.hasToastFn !== 'function' || state.hasInlineFn !== 'function') {
            throw new Error(JSON.stringify(state));
        }

        const behavior = await page.evaluate(async () => {
            const results = {
                plainClickBeforeConfirm: null,
                plainClickAfterConfirm: null,
                plainHtmxCount: 0,
                explicitHtmxBeforeConfirm: null,
                explicitHtmxAfterConfirm: null,
                explicitClickAfterConfirm: null,
                implicitHtmxAfterConfirm: null,
                implicitClickAfterConfirm: null,
                submitBeforeConfirm: null,
                submitAfterConfirm: null,
                requestSubmitWithSubmitterBeforeConfirm: null,
                requestSubmitWithSubmitterAfterConfirm: null,
                requestSubmitWithoutSubmitterBeforeConfirm: null,
                requestSubmitWithoutSubmitterAfterConfirm: null,
                requestSubmitWithSubmitterAfterNoSubmitterBeforeConfirm: null,
                requestSubmitWithSubmitterAfterNoSubmitterAfterConfirm: null
            };

            const originalHtmx = window.htmx;
            const htmxEvents = [];
            window.htmx = {
                trigger(target, eventName) {
                    htmxEvents.push({
                        id: target.id,
                        eventName
                    });
                }
            };

            const flush = () => new Promise(resolve => window.setTimeout(resolve, 0));

            try {
                const plainButton = document.createElement('button');
                plainButton.type = 'button';
                plainButton.id = 'plain-confirm-button';
                plainButton.dataset.confirmMessage = 'plain click';
                let plainClickCount = 0;
                plainButton.addEventListener('click', () => {
                    plainClickCount += 1;
                });
                document.body.appendChild(plainButton);

                plainButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
                await flush();
                results.plainClickBeforeConfirm = plainClickCount;
                results.plainHtmxCount = htmxEvents.length;
                window.closeAppDialog(true);
                await flush();
                results.plainClickAfterConfirm = plainClickCount;

                const explicitHtmxButton = document.createElement('button');
                explicitHtmxButton.type = 'button';
                explicitHtmxButton.id = 'explicit-htmx-button';
                explicitHtmxButton.dataset.confirmMessage = 'explicit htmx';
                explicitHtmxButton.setAttribute('hx-trigger', 'confirmed-action');
                let explicitClickCount = 0;
                explicitHtmxButton.addEventListener('click', () => {
                    explicitClickCount += 1;
                });
                document.body.appendChild(explicitHtmxButton);

                explicitHtmxButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
                await flush();
                results.explicitHtmxBeforeConfirm = htmxEvents.length;
                window.closeAppDialog(true);
                await flush();
                results.explicitHtmxAfterConfirm = htmxEvents.length;
                results.explicitClickAfterConfirm = explicitClickCount;

                const implicitClickButton = document.createElement('button');
                implicitClickButton.type = 'button';
                implicitClickButton.id = 'implicit-click-button';
                implicitClickButton.dataset.confirmMessage = 'implicit click';
                let implicitClickCount = 0;
                implicitClickButton.addEventListener('click', () => {
                    implicitClickCount += 1;
                });
                document.body.appendChild(implicitClickButton);

                implicitClickButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
                await flush();
                window.closeAppDialog(true);
                await flush();
                results.implicitHtmxAfterConfirm = htmxEvents.length;
                results.implicitClickAfterConfirm = implicitClickCount;

                const submitForm = document.createElement('form');
                submitForm.id = 'submit-confirm-form';
                const submitButton = document.createElement('button');
                submitButton.type = 'submit';
                submitButton.dataset.confirmMessage = 'submitter confirm';
                submitButton.textContent = 'submit';
                let submitCount = 0;
                submitForm.addEventListener('submit', event => {
                    event.preventDefault();
                    submitCount += 1;
                });
                submitForm.appendChild(submitButton);
                document.body.appendChild(submitForm);

                submitButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
                await flush();
                results.submitBeforeConfirm = submitCount;
                window.closeAppDialog(true);
                await flush();
                results.submitAfterConfirm = submitCount;

                const requestSubmitWithSubmitterForm = document.createElement('form');
                requestSubmitWithSubmitterForm.id = 'request-submit-with-submitter-form';
                const requestSubmitInput = document.createElement('input');
                requestSubmitInput.name = 'query';
                const requestSubmitButton = document.createElement('button');
                requestSubmitButton.type = 'submit';
                requestSubmitButton.dataset.confirmMessage = 'request submit confirm';
                requestSubmitButton.textContent = 'go';
                let requestSubmitWithSubmitterCount = 0;
                requestSubmitWithSubmitterForm.addEventListener('submit', event => {
                    event.preventDefault();
                    requestSubmitWithSubmitterCount += 1;
                });
                requestSubmitWithSubmitterForm.appendChild(requestSubmitInput);
                requestSubmitWithSubmitterForm.appendChild(requestSubmitButton);
                document.body.appendChild(requestSubmitWithSubmitterForm);

                requestSubmitWithSubmitterForm.requestSubmit(requestSubmitButton);
                await flush();
                results.requestSubmitWithSubmitterBeforeConfirm = requestSubmitWithSubmitterCount;
                window.closeAppDialog(true);
                await flush();
                results.requestSubmitWithSubmitterAfterConfirm = requestSubmitWithSubmitterCount;

                const requestSubmitWithoutSubmitterForm = document.createElement('form');
                requestSubmitWithoutSubmitterForm.id = 'request-submit-without-submitter-form';
                const requestSubmitWithoutSubmitterInput = document.createElement('input');
                requestSubmitWithoutSubmitterInput.name = 'term';
                const requestSubmitWithoutSubmitterButton = document.createElement('button');
                requestSubmitWithoutSubmitterButton.type = 'submit';
                requestSubmitWithoutSubmitterButton.dataset.confirmMessage = 'should not be inferred';
                requestSubmitWithoutSubmitterButton.textContent = 'plain';
                let requestSubmitWithoutSubmitterCount = 0;
                requestSubmitWithoutSubmitterForm.addEventListener('submit', event => {
                    event.preventDefault();
                    requestSubmitWithoutSubmitterCount += 1;
                });
                requestSubmitWithoutSubmitterForm.appendChild(requestSubmitWithoutSubmitterInput);
                requestSubmitWithoutSubmitterForm.appendChild(requestSubmitWithoutSubmitterButton);
                document.body.appendChild(requestSubmitWithoutSubmitterForm);

                requestSubmitWithoutSubmitterForm.requestSubmit();
                await flush();
                results.requestSubmitWithoutSubmitterBeforeConfirm = requestSubmitWithoutSubmitterCount;
                results.requestSubmitWithoutSubmitterAfterConfirm = requestSubmitWithoutSubmitterCount;

                requestSubmitWithoutSubmitterButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
                await flush();
                results.requestSubmitWithSubmitterAfterNoSubmitterBeforeConfirm = requestSubmitWithoutSubmitterCount;
                window.closeAppDialog(true);
                await flush();
                results.requestSubmitWithSubmitterAfterNoSubmitterAfterConfirm = requestSubmitWithoutSubmitterCount;

                return results;
            } finally {
                window.htmx = originalHtmx;
            }
        });

        if (
            behavior.plainClickBeforeConfirm !== 0 ||
            behavior.plainClickAfterConfirm !== 1 ||
            behavior.plainHtmxCount !== 0 ||
            behavior.explicitHtmxBeforeConfirm !== 0 ||
            behavior.explicitHtmxAfterConfirm !== 1 ||
            behavior.explicitClickAfterConfirm !== 0 ||
            behavior.implicitHtmxAfterConfirm !== 1 ||
            behavior.implicitClickAfterConfirm !== 1 ||
            behavior.submitBeforeConfirm !== 0 ||
            behavior.submitAfterConfirm !== 1 ||
            behavior.requestSubmitWithSubmitterBeforeConfirm !== 0 ||
            behavior.requestSubmitWithSubmitterAfterConfirm !== 1 ||
            behavior.requestSubmitWithoutSubmitterBeforeConfirm !== 1 ||
            behavior.requestSubmitWithoutSubmitterAfterConfirm !== 1 ||
            behavior.requestSubmitWithSubmitterAfterNoSubmitterBeforeConfirm !== 1 ||
            behavior.requestSubmitWithSubmitterAfterNoSubmitterAfterConfirm !== 2
        ) {
            throw new Error(JSON.stringify(behavior));
        }

        async function expectAppDialog(url, triggerSelector, expectedTitle, expectedMessage) {
            const dialogsBefore = nativeDialogs.length;

            await page.goto(url);
            await page.click(triggerSelector);
            await page.waitForFunction(() => {
                const overlay = document.getElementById('app-dialog-root');
                return overlay?.classList.contains('active') && overlay.getAttribute('aria-hidden') === 'false';
            });

            const dialogState = await page.evaluate(() => ({
                title: document.getElementById('app-dialog-title')?.textContent?.trim() ?? '',
                message: document.getElementById('app-dialog-message')?.textContent?.trim() ?? '',
                confirmLabel: document.querySelector('[data-dialog-confirm]')?.textContent?.trim() ?? '',
                cancelLabel: document.querySelector('[data-dialog-cancel]')?.textContent?.trim() ?? ''
            }));

            if (nativeDialogs.length !== dialogsBefore) {
                throw new Error(`native dialog opened on ${url}: ${nativeDialogs.slice(dialogsBefore).join(' | ')}`);
            }

            if (
                dialogState.title !== expectedTitle ||
                dialogState.message !== expectedMessage ||
                dialogState.confirmLabel !== '削除する' ||
                dialogState.cancelLabel !== 'キャンセル'
            ) {
                throw new Error(`unexpected app dialog on ${url}: ${JSON.stringify(dialogState)}`);
            }

            await page.click('[data-dialog-cancel]');
            await page.waitForFunction(() => {
                const overlay = document.getElementById('app-dialog-root');
                return overlay && !overlay.classList.contains('active') && overlay.getAttribute('aria-hidden') === 'true';
            });
        }

        const spaceDetailUrl = await (async () => {
            await page.goto('http://127.0.0.1:8080/spaces');
            return page.$eval('tr[data-space-id] td a[href^="/spaces/"]', link => link.href);
        })();

        const workerDefinitionDetailUrl = await (async () => {
            await page.goto('http://127.0.0.1:8080/worker-definitions');
            return page.$eval('a[href^="/worker-definitions/"]:not([href$="/edit"]):not([href$="/new"])', link => link.href);
        })();

        await expectAppDialog(
            'http://127.0.0.1:8080/jobs/local-echo-job',
            'button[hx-delete="/api/jobs/local-echo-job"]',
            'ジョブを削除しますか？',
            'このジョブ定義を削除します。元に戻せません。'
        );

        await expectAppDialog(
            'http://127.0.0.1:8080/spaces',
            'form[action^="/spaces/"][action$="/delete"] button[type="submit"]',
            'スペースを削除しますか？',
            '所属していたジョブは自動的かつ安全に「未分類」に移動します。'
        );

        await expectAppDialog(
            spaceDetailUrl,
            'form[action^="/spaces/"][action$="/delete"] button[type="submit"]',
            'スペースを削除しますか？',
            '所属していたジョブは自動的かつ安全に「未分類」に移動します。'
        );

        await expectAppDialog(
            workerDefinitionDetailUrl,
            'form[action^="/worker-definitions/"][action$="/delete"] button[type="submit"]',
            'ワーカー定義を削除しますか？',
            '紐付いているジョブの実行に影響が出る可能性があります。'
        );

        async function expectToastFromHtmxHandler(url, triggerSelector, expectedMessage, options = {}) {
            const dialogsBefore = nativeDialogs.length;
            const expectRenderedToast = options.expectRenderedToast !== false;

            await page.goto(url);
            const triggerState = await page.$eval(triggerSelector, trigger => ({
                onclick: trigger.getAttribute('onclick') ?? '',
                afterRequest: trigger.getAttribute('hx-on::after-request') ?? ''
            }));

            if (!triggerState.afterRequest.includes('window.appToast') || !triggerState.afterRequest.includes(expectedMessage)) {
                throw new Error(`missing toast handler on ${url}: ${JSON.stringify(triggerState)}`);
            }

            if (triggerState.onclick.includes('alert(')) {
                throw new Error(`native alert handler still present on ${url}: ${JSON.stringify(triggerState)}`);
            }

            await page.evaluate(() => {
                const region = document.getElementById('app-toast-region');
                if (region) {
                    region.replaceChildren();
                }
            });
            await page.click(triggerSelector);
            let toastState = { count: 0, message: '' };
            if (expectRenderedToast) {
                await page.waitForFunction(() => {
                    const region = document.getElementById('app-toast-region');
                    return (region?.children.length ?? 0) > 0;
                }, { timeout: 5000 });

                toastState = await page.evaluate(() => {
                    const region = document.getElementById('app-toast-region');
                    const toast = region?.firstElementChild;
                    return {
                        count: region?.children.length ?? 0,
                        message: toast?.textContent?.trim() ?? ''
                    };
                });
            } else {
                await new Promise(resolve => setTimeout(resolve, 300));
            }

            if (nativeDialogs.length !== dialogsBefore) {
                throw new Error(`native dialog opened during toast check on ${url}: ${nativeDialogs.slice(dialogsBefore).join(' | ')}`);
            }

            if (expectRenderedToast && (toastState.count === 0 || toastState.message !== expectedMessage)) {
                throw new Error(`unexpected toast on ${url}: ${JSON.stringify(toastState)}`);
            }
        }

        await expectToastFromHtmxHandler(
            'http://127.0.0.1:8080/jobs',
            'button[hx-post=\"/api/jobs/local-echo-job/trigger\"]',
            'ジョブ実行をトリガーしました。'
        );

        await expectToastFromHtmxHandler(
            'http://127.0.0.1:8080/jobs/local-echo-job',
            'button[hx-post=\"/api/jobs/local-echo-job/trigger\"]',
            'ジョブ実行をトリガーしました。',
            { expectRenderedToast: false }
        );

        await expectToastFromHtmxHandler(
            spaceDetailUrl,
            'button[hx-post^=\"/api/jobs/\"][hx-swap=\"none\"]',
            'ジョブ実行をトリガーしました。',
            { expectRenderedToast: false }
        );

        await page.goto('http://127.0.0.1:8080/jobs/local-echo-job?tab=runs');
        const runsPollingState = await page.evaluate(() => {
            const container = document.getElementById('runs-table-container');
            return {
                hxTrigger: container?.getAttribute('hx-trigger') ?? '',
                hxGet: container?.getAttribute('hx-get') ?? '',
                pollUrl: container?.getAttribute('data-poll-url') ?? ''
            };
        });
        if (
            runsPollingState.hxTrigger !== 'every 5s' ||
            runsPollingState.hxGet !== '/jobs/local-echo-job/runs' ||
            runsPollingState.pollUrl !== '/jobs/local-echo-job/runs'
        ) {
            throw new Error(`unexpected runs polling state: ${JSON.stringify(runsPollingState)}`);
        }

        await page.click('.ec2-tab-btn[data-tab="tab-env"]');
        const envPollingState = await page.evaluate(() => {
            const container = document.getElementById('runs-table-container');
            return {
                hxTrigger: container?.getAttribute('hx-trigger') ?? '',
                hxGet: container?.getAttribute('hx-get') ?? ''
            };
        });
        if (envPollingState.hxTrigger !== '' || envPollingState.hxGet !== '/jobs/local-echo-job/runs') {
            throw new Error(`runs polling did not stop outside active tab: ${JSON.stringify(envPollingState)}`);
        }

        async function expectInlineCompareNotice() {
            const dialogsBefore = nativeDialogs.length;

            await page.goto('http://127.0.0.1:8080/jobs/local-echo-job?tab=history');
            await page.click('input[name="compare-a"]:checked');
            await page.click('button[onclick="compareVersions()"]');
            await page.waitForFunction(() => {
                const panel = document.getElementById('job-history-panel');
                return !!panel?.querySelector('.inline-notice');
            });

            const noticeState = await page.evaluate(() => {
                const panel = document.getElementById('job-history-panel');
                const notice = panel?.querySelector('.inline-notice');
                return {
                    text: notice?.textContent?.trim() ?? ''
                };
            });

            if (nativeDialogs.length !== dialogsBefore) {
                throw new Error(`native dialog opened during compare notice check: ${nativeDialogs.slice(dialogsBefore).join(' | ')}`);
            }

            if (noticeState.text !== '比較する2つのバージョン（AとB）を選択してください。') {
                throw new Error(`unexpected compare notice: ${JSON.stringify(noticeState)}`);
            }
        }

        await expectInlineCompareNotice();
    } finally {
        await browser.close();
    }
})();
