<?php

namespace App;

class Noise13
{
    private function compute(): int
    {
        return 13;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
