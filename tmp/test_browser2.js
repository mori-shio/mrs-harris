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

    // Go to edit page
    await page.goto('http://127.0.0.1:8080/jobs/local-echo-job/edit');
    
    let invalidElements = await page.$$eval(':invalid', els => els.map(el => el.name || el.id));
    console.log("Initial invalid elements:", invalidElements);
    
    page.on('console', msg => console.log('PAGE LOG:', msg.text()));
    
    // Click submit
    try {
        const response = await Promise.all([
            page.waitForNavigation({ timeout: 2000 }).catch(() => null),
            page.click('button[type="submit"]')
        ]);
        
        console.log("Navigated?", !!response[0]);
        if (response[0]) {
            console.log("URL after submit:", page.url());
        }
    } catch(e) {
        console.log("Exception:", e.message);
    }
    
    invalidElements = await page.$$eval(':invalid', els => els.map(el => el.name || el.id));
    console.log("Invalid elements after submit:", invalidElements);
    
    await browser.close();
})();
