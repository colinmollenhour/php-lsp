<?php
namespace Acme\Tests\Integration;

use Acme\Core\Entity\User;
use Acme\Core\Repository\UserRepository;

class UserTest {
    public function testUserRepository(): void {
        $repo = new UserRepository();
        $user = new User(1, 'John', 'john@example.com');
        $repo->save($user);
        $found = $repo->findById(1);
        assert($found !== null && $found->name === 'John');
    }
}
