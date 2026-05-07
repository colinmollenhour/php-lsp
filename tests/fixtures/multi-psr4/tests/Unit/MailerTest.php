<?php
namespace Tests\Unit;

use App\Service\Mailer;
use Lib\Transport\SmtpClient;

class MailerTest {
    public function testSend(): void {
        $client = new SmtpClient();
        $mailer = new Mailer($client);
        $mailer->send('test@example.com', 'Subject', 'Body');
    }
}
