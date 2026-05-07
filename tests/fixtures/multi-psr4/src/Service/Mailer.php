<?php
namespace App\Service;

use Lib\Transport\SmtpClient;

class Mailer {
    public function __construct(
        private SmtpClient $client,
    ) {}

    public function send(string $to, string $subject, string $body): bool {
        return $this->client->deliver($to, $subject, $body);
    }
}
