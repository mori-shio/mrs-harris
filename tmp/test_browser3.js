const puppeteer = require('puppeteer-core');
(async () => {
    const browser = await puppeteer.launch({ 
        executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
        headless: "new" 
    });
    const page = await browser.newPage();
    
    // Login
    await page.goto('http://127.0.0.1:8080/login');
    await page.type('input[name="username"]', 'admin');
    await page.type('input[name="password"]', 'password');
    await Promise.all([
        page.waitForNavigation(),
        page.click('button[type="submit"]')
    ]);

    console.log("URL after login:", page.url());

    // Go to edit page
    await page.goto('http://127.0.0.1:8080/jobs/local-echo-job/edit');
    console.log("URL after goto edit:", page.url());
    
    const html = await page.content();
    if (html.includes("ジョブ 編集")) {
        console.log("Successfully on edit page");
    } else {
        console.log("Failed to load edit page");
    }

    let invalidElements = await page.$$eval(':invalid', els => els.map(el => el.name || el.id));
    console.log("Initial invalid elements on edit page:", invalidElements);
    
    // Fill something
    await page.type('textarea[name="description"]', ' tested');

    // Click submit
    try {
        const response = await Promise.all([
            page.waitForNavigation({ timeout: 3000 }).catch(() => null),
            page.click('button[type="submit"]')
        ]);
        
        if (response[0]) {
            console.log("URL after submit:", page.url());
        } else {
            console.log("Navigated? false");
        }
    } catch(e) {
        console.log("Exception:", e.message);
    }
    
    invalidElements = await page.$$eval(':invalid', els => els.map(el => el.name || el.id));
    console.log("Invalid elements after submit:", invalidElements);
    
    await browser.close();
})();
