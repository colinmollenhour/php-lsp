<?php
namespace Acme\Core\Entity;

class User {
    public function __construct(
        public readonly int $id,
        public readonly string $name,
        public readonly string $email,
    ) {}

    public function getDisplayName(): string {
        return $this->name;
    }
}
