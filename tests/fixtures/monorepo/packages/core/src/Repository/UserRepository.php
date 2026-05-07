<?php
namespace Acme\Core\Repository;

use Acme\Core\Entity\User;

class UserRepository {
    /** @var User[] */
    private array $users = [];

    public function findById(int $id): ?User {
        foreach ($this->users as $user) {
            if ($user->id === $id) {
                return $user;
            }
        }
        return null;
    }

    public function findAll(): array {
        return $this->users;
    }

    public function save(User $user): void {
        $this->users[$user->id] = $user;
    }
}
